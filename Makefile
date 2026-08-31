# Atajos de los gates de CI, para correrlos en local antes de commitear.
# Mismos nombres que en el proyecto LAMULA DRx, a propósito: quien salta entre
# los dos repositorios no tiene que aprender dos vocabularios.
#
#   make check   todo lo que corre el CI
#   make gen     regenera el contrato DSP↔RCP desde su esquema
#   make test    sólo los tests (Rust + Python + notebooks de oráculo)
#   make fmt     formatea las fuentes propias de Rust
#   make clean

PY ?= python3
CARGO ?= cargo

.PHONY: check gen lint test test-rust test-py test-oracles fmt clean

gen:
	$(PY) tools/gen_contract.py

# `--check` de regeneración: si el esquema y lo generado no coinciden, falla.
# `check_vendored_contract.py`: lo vendorizado del DRx no se ha tocado. No es
# paranoia — `cargo fmt` desciende por las declaraciones de módulo y ya
# reescribió ese fichero una vez.
lint:
	$(PY) tools/gen_contract.py --check
	$(PY) tools/check_vendored_contract.py
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets -- -D warnings

test-rust:
	$(CARGO) test

test-py:
	$(PY) -m pytest contract/tests -q

# Ejecuta de punta a punta cada notebook de oráculo (docs/algorithms/
# roadmap.md §"Método de estudio"): una celda que lanza AssertionError hace
# fallar nbconvert y por tanto el gate, no sólo se comprueba que el fichero
# parsee.
test-oracles:
	$(PY) -m jupyter nbconvert --to notebook --execute --stdout \
		tools/oracles/*.ipynb > /dev/null

test: test-rust test-py test-oracles

# rustfmt sólo sobre las fuentes propias. `cargo fmt` a secas también vale
# gracias al `rustfmt::skip` de la declaración del módulo vendorizado, pero
# apuntar explícito deja claro qué se formatea y qué no.
fmt:
	$(CARGO) fmt

check: lint test

clean:
	$(CARGO) clean
	rm -rf .pytest_cache contract/tests/__pycache__ contract/generated/__pycache__
