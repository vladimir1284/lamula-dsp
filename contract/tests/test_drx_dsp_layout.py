"""Test de disposición del contrato DRx↔DSP v0.1, lado DSP.

El proyecto DRx verifica su propio codegen comparando byte a byte la salida de C
contra la de Python, pero deja explícitamente fuera el lado Rust porque en aquel
repositorio no hay toolchain de Rust. Esa verificación es responsabilidad de
este proyecto, y este fichero es la mitad que se puede correr sin compilador:
comprueba que la disposición vendorizada sigue siendo la que el DSP espera.

La idea central es que la disposición se **reescribe aquí de forma
independiente**, campo por campo, en vez de leerse del módulo generado. Un test
que se limitara a comprobar `Ray.SIZE == Ray.SIZE` no detecta nada. Al fijar
aquí el nombre, el tipo y el desplazamiento de cada campo, un reordenamiento
aguas arriba, un cambio de tipo o un campo insertado rompen el test aunque el
tamaño total no se mueva — que es justo el fallo silencioso que arruinaría la
interpretación de los rayos en producción.

Cuando exista el crate de Rust, el test hermano (`contract/tests/drx_layout.rs`)
comprobará con `size_of` y `offset_of` estos mismos números contra el struct
`#[repr(C, packed)]`, cerrando el hueco que el proyecto DRx dejó abierto.
"""

from __future__ import annotations

import importlib.util
import re
import struct
import sys
import tomllib
from pathlib import Path

import pytest

VENDOR = Path(__file__).resolve().parent.parent / "vendor"

# Tamaño en bytes de cada código de tipo del contrato. El contrato es
# little-endian y empaquetado, así que el desplazamiento de un campo es la suma
# de los tamaños de los que le preceden: no hay relleno implícito que calcular.
WIDTH = {"u8": 1, "u16": 2, "u32": 4, "u64": 8}

# Código de `struct` correspondiente, para reconstruir la cadena de formato y
# contrastarla con la que trae el módulo generado.
STRUCT_CODE = {"u8": "B", "u16": "H", "u32": "I", "u64": "Q"}

# --- Disposición esperada, reescrita a mano desde el contrato v0.1. ---------
#
# Fuente: lamula-drx, `contract/schema/drx_dsp_v0_1.toml`, congelado por D-08.
# Si un cambio aguas arriba rompe este test, la corrección es actualizar estas
# tablas *a la vez* que se sube el pin de `vendor/UPSTREAM.toml` y se revisa el
# código de ingesta — nunca sólo el pin.

EXPECTED_HEADER = [
    ("magic", "u32"),
    ("version_major", "u8"),
    ("version_minor", "u8"),
    ("msg_type", "u8"),
    ("flags", "u8"),
    ("payload_len", "u32"),
]

EXPECTED_RAY = [
    ("seq", "u32"),
    ("timestamp_ns", "u64"),
    ("trigger_count", "u32"),
    ("azimuth_raw", "u32"),
    ("elevation_raw", "u32"),
    ("prf_div", "u32"),
    ("bins", "u16"),
    ("pulse_width_idx", "u8"),
    ("pulse_mode", "u8"),
    ("cell_mode", "u8"),
    ("n_channels", "u8"),
    ("channel_mask", "u8"),
    ("ray_flags", "u8"),
]

EXPECTED_STATUS = [
    ("uptime_s", "u32"),
    ("bite_flags", "u32"),
    ("ssa_underruns", "u32"),
    ("dma_overruns", "u32"),
    ("ssi_errors", "u32"),
    ("ddc_overflows", "u32"),
    ("last_error", "u8"),
    ("pad0", "u8"),
    ("pad1", "u16"),
]

EXPECTED_CONFIG = [
    ("seq", "u32"),
    ("prf_div", "u32"),
    ("range_bins", "u16"),
    ("pulse_width_idx", "u8"),
    ("pulse_mode", "u8"),
    ("cell_mode", "u8"),
    ("channel_mask", "u8"),
    ("scan_mode", "u8"),
    ("pad0", "u8"),
    ("trigger_delay_0", "u32"),
    ("trigger_delay_1", "u32"),
    ("trigger_delay_2", "u32"),
    ("trigger_delay_3", "u32"),
    ("trigger_width_0", "u32"),
    ("trigger_width_1", "u32"),
    ("trigger_width_2", "u32"),
    ("trigger_width_3", "u32"),
]

EXPECTED_CONFIG_ACK = [
    ("seq", "u32"),
    ("error", "u8"),
    ("pad0", "u8"),
    ("pad1", "u16"),
]

EXPECTED_AFC = [
    ("nco_phase_inc", "u64"),
    ("apply_at_seq", "u32"),
    ("pad0", "u32"),
]

# Tamaño total esperado, escrito a mano y no derivado de las tablas de arriba:
# si alguien edita una tabla sin pensar, el tamaño lo delata.
EXPECTED_SIZES = {
    "Header": 12,
    "Ray": 36,
    "Status": 28,
    "Config": 48,
    "ConfigAck": 8,
    "Afc": 16,
}

MESSAGES = {
    "Header": EXPECTED_HEADER,
    "Ray": EXPECTED_RAY,
    "Status": EXPECTED_STATUS,
    "Config": EXPECTED_CONFIG,
    "ConfigAck": EXPECTED_CONFIG_ACK,
    "Afc": EXPECTED_AFC,
}

# Constantes de identidad del contrato. `magic` es "LMDR" en ASCII
# little-endian; que coincida es lo primero que mira el ingestor antes de
# fiarse de un byte del flujo.
EXPECTED_MAGIC = 0x4C4D4452
EXPECTED_VERSION = (0, 1)

# Identificadores de mensaje. El sentido va anotado porque el DSP sólo debe
# emitir los "down" y sólo debe aceptar los "up": un identificador que cambiara
# de sentido aguas arriba es un fallo de contrato, no de transporte.
EXPECTED_MSG_TYPES = {
    "RAY": (1, "up"),
    "STATUS": (2, "up"),
    "CONFIG": (3, "down"),
    "CONFIG_ACK": (4, "up"),
    "AFC": (5, "down"),
}


def _offsets(fields):
    """Desplazamiento de cada campo, asumiendo empaquetado sin relleno implícito."""
    offset = 0
    out = []
    for name, kind in fields:
        out.append((name, kind, offset))
        offset += WIDTH[kind]
    return out, offset


@pytest.fixture(scope="module")
def generated():
    """Carga el módulo Python vendorizado por ruta, sin instalarlo como paquete."""
    path = VENDOR / "drx_dsp_v0_1.py"
    if not path.exists():
        pytest.fail(
            f"falta {path}. Se vendoriza desde lamula-drx; ver vendor/UPSTREAM.toml."
        )
    spec = importlib.util.spec_from_file_location("drx_dsp_v0_1", path)
    module = importlib.util.module_from_spec(spec)
    # El módulo generado usa `from __future__ import annotations`, así que sus
    # dataclasses guardan las anotaciones como cadenas y `dataclasses` las
    # resuelve buscando el módulo en `sys.modules`. Sin registrarlo antes de
    # ejecutarlo, la propia definición de la primera dataclass revienta.
    sys.modules[spec.name] = module
    try:
        spec.loader.exec_module(module)
    except Exception:
        del sys.modules[spec.name]
        raise
    return module


@pytest.fixture(scope="module")
def rust_source():
    path = VENDOR / "drx_dsp_v0_1.rs"
    if not path.exists():
        pytest.fail(
            f"falta {path}. Se vendoriza desde lamula-drx; ver vendor/UPSTREAM.toml."
        )
    return path.read_text(encoding="utf-8")


@pytest.fixture(scope="module")
def pin():
    return tomllib.loads((VENDOR / "UPSTREAM.toml").read_text(encoding="utf-8"))


# --- Identidad del contrato -------------------------------------------------


def test_magic_y_version(generated, pin):
    assert generated.MAGIC == EXPECTED_MAGIC
    assert (generated.VERSION_MAJOR, generated.VERSION_MINOR) == EXPECTED_VERSION
    # El pin tiene que hablar de la misma versión que el código vendorizado.
    assert pin["contract"]["version_major"] == EXPECTED_VERSION[0]
    assert pin["contract"]["version_minor"] == EXPECTED_VERSION[1]
    assert pin["contract"]["magic"] == EXPECTED_MAGIC


def test_tipos_de_mensaje(generated):
    for name, (value, _dir) in EXPECTED_MSG_TYPES.items():
        assert getattr(generated.MsgType, name) == value, f"msg_type {name} cambió"
    # Ningún identificador nuevo sin revisar el ingestor: un mensaje "up"
    # desconocido no se puede ignorar en silencio.
    declarados = {
        n for n in vars(generated.MsgType) if n.isupper() and not n.startswith("_")
    }
    assert declarados == set(EXPECTED_MSG_TYPES)


# --- Disposición, lado Python ----------------------------------------------


@pytest.mark.parametrize("nombre", sorted(MESSAGES))
def test_tamano_python(generated, nombre):
    assert getattr(generated, nombre).SIZE == EXPECTED_SIZES[nombre]


@pytest.mark.parametrize("nombre", sorted(MESSAGES))
def test_tamano_coherente_con_los_campos(nombre):
    """Los campos reescritos aquí suman el tamaño reescrito aquí."""
    _, total = _offsets(MESSAGES[nombre])
    assert total == EXPECTED_SIZES[nombre]


@pytest.mark.parametrize("nombre", sorted(MESSAGES))
def test_orden_de_campos_python(generated, nombre):
    esperado = tuple(name for name, _ in MESSAGES[nombre])
    assert getattr(generated, nombre).FIELDS == esperado


@pytest.mark.parametrize("nombre", sorted(MESSAGES))
def test_formato_struct_python(generated, nombre):
    """La cadena de `struct` se reconstruye desde los tipos, no se copia."""
    esperado = "<" + "".join(STRUCT_CODE[kind] for _, kind in MESSAGES[nombre])
    assert getattr(generated, nombre).FORMAT == esperado
    assert struct.calcsize(esperado) == EXPECTED_SIZES[nombre]


@pytest.mark.parametrize("nombre", sorted(MESSAGES))
def test_desplazamientos_efectivos(generated, nombre):
    """Cada campo aterriza donde se espera, comprobado empaquetando de verdad.

    Se pone un valor distinto por campo y se lee de vuelta desde el
    desplazamiento calculado a mano. Detecta el reordenamiento de dos campos del
    mismo ancho, que ni el tamaño total ni la cadena de formato delatan.
    """
    cls = getattr(generated, nombre)
    campos, _ = _offsets(MESSAGES[nombre])

    valores = {}
    for indice, (name, kind, _off) in enumerate(campos, start=1):
        # Un valor único por campo que quepa en su ancho.
        valores[name] = indice % (1 << (8 * WIDTH[kind]))

    empaquetado = cls(**valores).pack()
    assert len(empaquetado) == EXPECTED_SIZES[nombre]

    for name, kind, offset in campos:
        crudo = empaquetado[offset : offset + WIDTH[kind]]
        leido = int.from_bytes(crudo, "little")
        assert leido == valores[name], (
            f"{nombre}.{name} no está en el desplazamiento {offset}"
        )


# --- Disposición, lado Rust (sin compilador) --------------------------------
#
# No hay toolchain de Rust todavía, así que estas comprobaciones son textuales
# sobre el fichero generado. Son una red, no la verificación definitiva: el test
# hermano en Rust con `offset_of` la sustituye en cuanto exista el crate.

RUST_STRUCT = re.compile(
    r"pub struct (?P<name>\w+) \{(?P<body>.*?)\n\}", re.DOTALL
)
RUST_FIELD = re.compile(r"^\s{4}pub (?P<name>\w+): (?P<type>\w+),$", re.MULTILINE)


def _rust_structs(source):
    return {
        m.group("name"): [
            (f.group("name"), f.group("type"))
            for f in RUST_FIELD.finditer(m.group("body"))
        ]
        for m in RUST_STRUCT.finditer(source)
    }


@pytest.mark.parametrize("nombre", sorted(MESSAGES))
def test_campos_rust(rust_source, nombre):
    structs = _rust_structs(rust_source)
    assert nombre in structs, f"el .rs vendorizado no declara {nombre}"
    assert structs[nombre] == MESSAGES[nombre]


@pytest.mark.parametrize("nombre", sorted(MESSAGES))
def test_constantes_de_tamano_rust(rust_source, nombre):
    # Header -> HEADER_SIZE, ConfigAck -> CONFIG_ACK_SIZE.
    const = re.sub(r"(?<!^)(?=[A-Z])", "_", nombre).upper() + "_SIZE"
    encontrado = re.search(
        rf"pub const {const}: usize = (\d+);", rust_source
    )
    assert encontrado, f"el .rs vendorizado no declara {const}"
    assert int(encontrado.group(1)) == EXPECTED_SIZES[nombre]


def test_structs_rust_empaquetados(rust_source):
    """Sin `repr(C, packed)` la disposición la decide el compilador, no el contrato."""
    for nombre in MESSAGES:
        bloque = rust_source.split(f"pub struct {nombre} {{")[0]
        atributos = bloque.rsplit("///", 1)[-1] if "///" in bloque else bloque
        assert "#[repr(C, packed)]" in atributos[-400:], (
            f"{nombre} no está declarado #[repr(C, packed)]"
        )
