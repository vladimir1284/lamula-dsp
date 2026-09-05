#!/usr/bin/env bash
# Empaquetado offline/air-gapped del binario `lamula-dsp` (`docs/dsp-plan.md`
# §3.1 "Packaging: offline, air-gapped installer for the Linux SBC (systemd
# service); reproducible offline build").
#
# Dos fases, deliberadamente separadas: (1) en una máquina CON acceso a
# internet, vendoriza las dependencias en un árbol reproducible; (2) en la
# máquina de destino (o una idéntica, sin red), compila SOLO desde ese árbol
# vendorizado, sin tocar crates.io. La fase 2 es la que se corre en el SBC
# real o en su imagen de build.
#
# Uso:
#   tools/offline_build.sh vendor   # fase 1: con red, genera vendor/ + .cargo/config.toml.offline
#   tools/offline_build.sh build    # fase 2: sin red, --offline, produce target/release/lamula-dsp
#
# Lo que este script NO decide, porque este repositorio tampoco lo ha
# decidido todavía (`docs/dsp-plan.md` §8.2 Fase 0, "CPU/SBC target
# undecided"): arquitectura de destino (ARM vs x86), toolchain de
# cross-compilación (`cross`/`cargo-zigbuild`), ni el musl-vs-glibc del
# binario estático que el plan menciona como opción. Compila para el host
# actual tal cual — adaptar `--target` aquí cuando Fase 0 cierre esa
# decisión.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

VENDOR_DIR="vendor"
OFFLINE_CARGO_CONFIG=".cargo/config.toml.offline"

usage() {
    echo "uso: $0 {vendor|build}" >&2
    exit 1
}

cmd_vendor() {
    echo "vendorizando dependencias en ${VENDOR_DIR}/ ..."
    mkdir -p .cargo
    cargo vendor "${VENDOR_DIR}" > "${OFFLINE_CARGO_CONFIG}"
    cat <<EOF

Listo. Para transportar a la máquina air-gapped, copiar:
  - todo el repositorio (código + Cargo.lock)
  - ${VENDOR_DIR}/
  - ${OFFLINE_CARGO_CONFIG}

En la máquina de destino, antes de "$0 build", activar la config offline:
  cp ${OFFLINE_CARGO_CONFIG} .cargo/config.toml
EOF
}

cmd_build() {
    if [ ! -d "${VENDOR_DIR}" ]; then
        echo "error: falta ${VENDOR_DIR}/ — correr \"$0 vendor\" primero (con red) y transportarlo" >&2
        exit 1
    fi
    if [ ! -f ".cargo/config.toml" ]; then
        echo "error: falta .cargo/config.toml — copiar ${OFFLINE_CARGO_CONFIG} a .cargo/config.toml" >&2
        exit 1
    fi
    echo "compilando lamula-dsp --release --offline ..."
    cargo build --release --offline -p lamula-dsp-service
    echo "binario en target/release/lamula-dsp"
}

case "${1:-}" in
    vendor) cmd_vendor ;;
    build) cmd_build ;;
    *) usage ;;
esac
