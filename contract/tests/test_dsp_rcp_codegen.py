"""Acuerdo entre las implementaciones generadas del contrato DSP↔RCP.

El codegen produce tres implementaciones de la misma fuente. Que las tres
*existan* no prueba nada; lo que hay que probar es que producen **los mismos
bytes**. Este fichero compara la de Python con la de TypeScript campo por campo
y estructura por estructura.

Por qué ese par y no otro: la implementación de Rust son estructuras
`#[repr(C, packed)]` sin código de serialización, así que su disposición queda
completamente determinada por tamaño y desplazamientos, y eso ya lo comprueba
`crates/contract/tests/layout.rs` contra el compilador. La de TypeScript, en
cambio, es aritmética de índices generada: cada `setFloat32(base + 44, ...)` es
una línea que puede estar mal sin que nada más lo note. Es la que necesita una
comparación de bytes de verdad.

Supuesto explícito: máquina little-endian. El contrato lo es, y el test compara
el hexadecimal producido por dos codificadores que ambos fuerzan little-endian,
así que el resultado no depende del anfitrión; lo que sí lo asume es la
equivalencia entre la disposición de Rust y estos bytes.
"""

from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
GENERATED = HERE.parent / "generated"
NODE_DRIVER = HERE / "encode_ts.mjs"

# Estructuras a comparar, con el prefijo de sus constantes de tamaño.
# `MomentField` entra aunque no viaje suelta: va incrustada en la carga útil de
# un radial, que es justo donde un desplazamiento mal calculado haría el daño.
STRUCTS = {
    "Header": "HEADER",
    "MomentRay": "MOMENT_RAY",
    "MomentField": "MOMENT_FIELD",
    "SpectrumFrame": "SPECTRUM_FRAME",
    "Status": "STATUS",
    "BiteEvent": "BITE_EVENT",
    "ConfigAck": "CONFIG_ACK",
    "SelftestResult": "SELFTEST_RESULT",
    "Capabilities": "CAPABILITIES",
    "Config": "CONFIG",
    "Control": "CONTROL",
    "SelftestRequest": "SELFTEST_REQUEST",
}

# Anchos por código de `struct`, para saber cuánto cabe en cada campo entero.
WIDTH_BY_CODE = {"B": 1, "H": 2, "I": 4, "Q": 8, "b": 1, "h": 2, "i": 4, "q": 8}
FLOAT_CODES = {"f", "d"}
BIG_CODES = {"Q", "q"}


def to_lower_camel(name: str) -> str:
    head, *rest = name.split("_")
    return head + "".join(part.capitalize() for part in rest)


@pytest.fixture(scope="module")
def generated():
    path = GENERATED / "dsp_rcp_v0_1.py"
    if not path.exists():
        pytest.fail(f"falta {path}. Ejecuta: python3 tools/gen_contract.py")
    spec = importlib.util.spec_from_file_location("dsp_rcp_v0_1", path)
    module = importlib.util.module_from_spec(spec)
    # Igual que con el contrato vendorizado: las dataclasses con anotaciones
    # diferidas necesitan el módulo en sys.modules para resolverlas.
    sys.modules[spec.name] = module
    try:
        spec.loader.exec_module(module)
    except Exception:
        del sys.modules[spec.name]
        raise
    return module


def _values_for(cls) -> dict[str, int | float]:
    """Un valor distinto y determinista por campo.

    Distinto por campo es lo que importa: con todo a cero, o con el mismo valor
    en todas partes, dos campos intercambiados producen bytes idénticos y el
    test no detecta nada.
    """
    codes = cls.FORMAT[1:]  # se quita el '<'
    values: dict[str, int | float] = {}
    for index, (name, code) in enumerate(zip(cls.FIELDS, codes), start=1):
        if code in FLOAT_CODES:
            # Representable exactamente en binario, para que la comparación no
            # dependa de por dónde redondea cada lenguaje.
            values[name] = index * 1.5 + 0.25
        else:
            values[name] = index % (1 << (8 * WIDTH_BY_CODE[code]))
    return values


def _json_ready(cls, values: dict[str, int | float]) -> dict[str, object]:
    """Pasa los enteros de 64 bits como cadena; JSON no los aguanta como número."""
    codes = cls.FORMAT[1:]
    out: dict[str, object] = {}
    for (name, code), value in zip(zip(cls.FIELDS, codes), values.values()):
        key = to_lower_camel(name)
        out[key] = str(value) if code in BIG_CODES else value
    return out


@pytest.fixture(scope="module")
def typescript_output(generated):
    node = shutil.which("node")
    if node is None:
        pytest.skip("no hay node; no se puede comparar con la implementación de TS")

    payload = {"structs": {}, "constNames": STRUCTS}
    for name in STRUCTS:
        cls = getattr(generated, name)
        payload["structs"][name] = _json_ready(cls, _values_for(cls))

    result = subprocess.run(
        [node, "--experimental-strip-types", str(NODE_DRIVER)],
        input=json.dumps(payload),
        capture_output=True,
        text=True,
        cwd=HERE,
    )
    if result.returncode != 0:
        pytest.fail(f"el codificador de TypeScript falló:\n{result.stderr}")
    return json.loads(result.stdout)


@pytest.mark.parametrize("name", sorted(STRUCTS))
def test_python_y_typescript_producen_los_mismos_bytes(
    generated, typescript_output, name
):
    cls = getattr(generated, name)
    esperado = cls(**_values_for(cls)).pack().hex()
    obtenido = typescript_output[name]
    assert obtenido == esperado, (
        f"{name}: Python y TypeScript no coinciden.\n"
        f"  python     {esperado}\n"
        f"  typescript {obtenido}"
    )


@pytest.mark.parametrize("name", sorted(STRUCTS))
def test_los_tamanos_coinciden_entre_lenguajes(generated, typescript_output, name):
    assert typescript_output["__sizes"][name] == getattr(generated, name).SIZE


def test_el_juego_de_valores_no_es_degenerado(generated):
    """Salvaguarda del propio test: valores repetidos lo volverían ciego.

    Si dos campos del mismo ancho recibieran el mismo valor, intercambiarlos no
    cambiaría los bytes y la comparación pasaría con un codegen roto.
    """
    for name in STRUCTS:
        cls = getattr(generated, name)
        values = list(_values_for(cls).values())
        assert len(set(values)) == len(values), f"{name}: valores repetidos"


def test_identidad_del_contrato(generated):
    assert generated.MAGIC == 0x4C4D4453, 'magic no es "LMDS"'
    assert (generated.VERSION_MAJOR, generated.VERSION_MINOR) == (0, 1)
