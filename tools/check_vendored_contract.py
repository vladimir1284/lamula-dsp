#!/usr/bin/env python3
"""Comprueba el ancla del contrato DRx↔DSP vendorizado.

El contrato `DRx↔DSP` lo posee el proyecto LAMULA DRx, que lo congeló en su fase
Z0. Aquí sólo se consume: `contract/vendor/` son copias byte a byte de su salida
generada. Este comprobador falla si:

  * un fichero de `vendor/` no coincide con el SHA-256 anotado en
    `vendor/UPSTREAM.toml` — es decir, alguien lo editó en local, o una
    herramienta lo reescribió sin querer. Ya ha pasado: `cargo fmt` desciende
    por las declaraciones de módulo y reformatea lo que encuentra;
  * el repositorio del DRx está accesible y su salida generada ha cambiado
    respecto a lo que se vendorizó, lo que significa que el contrato se movió
    aguas arriba y aquí nadie se enteró.

Lo segundo es una advertencia y no un fallo cuando el repositorio del DRx no
está montado: el CI de este proyecto no puede depender de que lo esté. Con
`--strict` sí falla, para el trabajo local donde ambos repositorios conviven.

Uso:
    python3 tools/check_vendored_contract.py
    python3 tools/check_vendored_contract.py --strict
"""

from __future__ import annotations

import argparse
import hashlib
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "contract" / "vendor"
PIN = VENDOR / "UPSTREAM.toml"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--strict",
        action="store_true",
        help="Falla, y no sólo advierte, si el repositorio del DRx no está o divergió.",
    )
    args = ap.parse_args()

    pin = tomllib.loads(PIN.read_text(encoding="utf-8"))
    errors: list[str] = []
    warnings: list[str] = []

    # 1. Los ficheros vendorizados son los que dice el ancla.
    for entry in pin["file"]:
        path = VENDOR / entry["path"]
        if not path.exists():
            errors.append(f"falta {path.relative_to(ROOT)}")
            continue
        actual = sha256(path)
        if actual != entry["sha256"]:
            errors.append(
                f"{path.relative_to(ROOT)} no coincide con el ancla\n"
                f"    esperado {entry['sha256']}\n"
                f"    obtenido {actual}\n"
                "    Nada de vendor/ se edita a mano. Si el cambio viene del DRx,"
                " re-vendoriza y actualiza UPSTREAM.toml."
            )

    # 2. El origen no se ha movido por debajo. Sólo se puede comprobar si el
    #    repositorio del DRx está montado al lado.
    upstream_root = (ROOT / pin["upstream"]["repo_path"]).resolve()
    if not upstream_root.is_dir():
        warnings.append(
            f"repositorio del DRx no encontrado en {upstream_root};"
            " no se comprueba divergencia con el origen"
        )
    else:
        watched = [(e["source"], e["sha256"]) for e in pin["file"]]
        watched += [(e["source"], e["sha256"]) for e in pin["watch"]]
        for source, expected in watched:
            path = upstream_root / source
            if not path.exists():
                warnings.append(f"el origen ya no tiene {source}")
                continue
            actual = sha256(path)
            if actual != expected:
                message = (
                    f"el origen cambió: {source}\n"
                    f"    vendorizado {expected}\n"
                    f"    origen      {actual}\n"
                    "    El contrato se movió aguas arriba. Re-vendoriza y sube el ancla."
                )
                (errors if args.strict else warnings).append(message)

    for warning in warnings:
        print(f"AVISO: {warning}", file=sys.stderr)
    for error in errors:
        print(f"ERROR: {error}", file=sys.stderr)

    if errors:
        return 1

    version = pin["contract"]
    print(
        "contrato vendorizado íntegro: "
        f"{pin['upstream']['project']} v{version['version_major']}.{version['version_minor']}"
        f" @ {pin['upstream']['commit'][:7]}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
