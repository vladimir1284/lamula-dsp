#!/usr/bin/env python3
"""Codegen del contrato DSP↔RCP.

Una sola fuente (`contract/schema/dsp_rcp_v0_1.toml`) genera las tres
implementaciones: Rust para el DSP, Python para el RCP y el banco de pruebas, y
TypeScript para el MMI. No se escribe ninguna a mano.

Este generador es un fork del `tools/gen_contract.py` del proyecto LAMULA DRx.
Se forkeó, y no se compartió, porque el plan del DSP (§6) asigna la propiedad de
este contrato a este proyecto: quien posee el contrato posee su generador. Las
diferencias respecto al original son tres, y todas salen de que aquí los
consumidores son otros:

  * Backend de TypeScript en lugar de backend de C. El MMI del RCP es Vue +
    TypeScript; aquí no hay ningún consumidor en C.
  * Tipos de coma flotante (f32/f64). El contrato del DRx los prohíbe por
    bit-exactitud entre un Cortex-R5 y Rust; en este enlace los dos extremos son
    IEEE-754 y el plan pide precisión plena. Ver la cabecera del esquema.
  * Mensajes con `dir = "payload"`, que van incrustados en la carga útil de otro
    mensaje y por tanto no entran en la enumeración de tipos de mensaje.

Uso:
    python3 tools/gen_contract.py
    python3 tools/gen_contract.py --check
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "contract" / "schema" / "dsp_rcp_v0_1.toml"
OUT_DIR = ROOT / "contract" / "generated"

BANNER_LINES = [
    "GENERADO por tools/gen_contract.py a partir de",
    "contract/schema/dsp_rcp_v0_1.toml. NO EDITAR A MANO.",
]

# (rust, struct, tamaño, python, DataView getter, DataView setter, tipo TS)
TYPES = {
    "u8": ("u8", "B", 1, "int", "getUint8", "setUint8", "number"),
    "u16": ("u16", "H", 2, "int", "getUint16", "setUint16", "number"),
    "u32": ("u32", "I", 4, "int", "getUint32", "setUint32", "number"),
    "u64": ("u64", "Q", 8, "int", "getBigUint64", "setBigUint64", "bigint"),
    "i8": ("i8", "b", 1, "int", "getInt8", "setInt8", "number"),
    "i16": ("i16", "h", 2, "int", "getInt16", "setInt16", "number"),
    "i32": ("i32", "i", 4, "int", "getInt32", "setInt32", "number"),
    "i64": ("i64", "q", 8, "int", "getBigInt64", "setBigInt64", "bigint"),
    "f32": ("f32", "f", 4, "float", "getFloat32", "setFloat32", "number"),
    "f64": ("f64", "d", 8, "float", "getFloat64", "setFloat64", "number"),
}

FLOAT_TYPES = {"f32", "f64"}

# Los tipos de 8 bytes se leen y escriben como BigInt en JavaScript: un u64 no
# cabe en el double de `number` sin perder enteros a partir de 2^53.
BIGINT_TYPES = {"u64", "i64"}


def load_schema() -> dict:
    with SCHEMA.open("rb") as handle:
        return tomllib.load(handle)


def size_of(fields: list[dict]) -> int:
    return sum(TYPES[f["type"]][2] for f in fields)


def struct_format(fields: list[dict]) -> str:
    return "<" + "".join(TYPES[f["type"]][1] for f in fields)


def offsets_of(fields: list[dict]) -> list[tuple[dict, int]]:
    """Desplazamiento de cada campo. Empaquetado: sin relleno implícito."""
    out = []
    offset = 0
    for field in fields:
        out.append((field, offset))
        offset += TYPES[field["type"]][2]
    return out


def has_float(fields: list[dict]) -> bool:
    return any(f["type"] in FLOAT_TYPES for f in fields)


def to_camel(name: str) -> str:
    return "".join(part.capitalize() for part in name.split("_"))


def to_lower_camel(name: str) -> str:
    head, *rest = name.split("_")
    return head + "".join(part.capitalize() for part in rest)


def wire_messages(schema: dict) -> list[dict]:
    """Mensajes que viajan sueltos, es decir, con su propio `msg_type`."""
    return [m for m in schema["message"] if m["dir"] != "payload"]


def all_structs(schema: dict) -> list[tuple[str, list[dict], str]]:
    """(nombre, campos, doc) de la cabecera y de todos los mensajes."""
    out = [("header", schema["header"]["fields"], "Cabecera común a todo mensaje.")]
    out += [(m["name"], m["fields"], m["doc"]) for m in schema["message"]]
    return out


# --- Rust -------------------------------------------------------------------


def render_rust(schema: dict) -> str:
    meta = schema["meta"]
    out = [f"// {line}" for line in BANNER_LINES]
    out += [
        "//",
        f"// Contrato DSP↔RCP v{meta['version_major']}.{meta['version_minor']} — lado DSP.",
        "//",
        "// Little-endian, empaquetado. Los asertos de tamaño y desplazamiento",
        "// viven en `contract/tests/dsp_rcp_layout.rs`; aquí van las constantes",
        "// de tamaño para que se puedan comprobar contra `size_of`.",
        "",
        "#![allow(dead_code)]",
        "",
        f"pub const MAGIC: u32 = 0x{meta['magic']:08X};",
        f"pub const VERSION_MAJOR: u8 = {meta['version_major']};",
        f"pub const VERSION_MINOR: u8 = {meta['version_minor']};",
        "",
    ]

    def emit_struct(name: str, fields: list[dict], doc: str) -> None:
        for line in doc.strip().splitlines():
            out.append(f"/// {line}" if line else "///")
        out.append("#[repr(C, packed)]")
        # `Eq` sólo cuando no hay coma flotante: f32 no implementa Eq, así que
        # derivarlo en una estructura con floats no compila. El contrato del DRx
        # no se topa con esto porque prohíbe los flotantes.
        derives = "Debug, Clone, Copy, PartialEq, Default"
        if not has_float(fields):
            derives = "Debug, Clone, Copy, PartialEq, Eq, Default"
        out.append(f"#[derive({derives})]")
        out.append(f"pub struct {to_camel(name)} {{")
        for field in fields:
            for line in field["doc"].strip().splitlines():
                out.append(f"    /// {line}" if line else "    ///")
            out.append(f"    pub {field['name']}: {TYPES[field['type']][0]},")
        out.append("}")
        out.append(f"pub const {name.upper()}_SIZE: usize = {size_of(fields)};")
        out.append("")

    emit_struct("header", schema["header"]["fields"], "Cabecera común a todo mensaje.")

    out.append("/// Tipos de mensaje que viajan sueltos por el cable.")
    out.append("#[repr(u8)]")
    out.append("#[derive(Debug, Clone, Copy, PartialEq, Eq)]")
    out.append("pub enum MsgType {")
    for message in wire_messages(schema):
        out.append(f"    /// {message['dir']}")
        out.append(f"    {to_camel(message['name'])} = {message['type_id']},")
    out.append("}")
    out.append("")

    for message in schema["message"]:
        emit_struct(message["name"], message["fields"], message["doc"])

    for enum in schema["enum"]:
        for line in enum["doc"].strip().splitlines():
            out.append(f"/// {line}" if line else "///")
        out.append(f"pub mod {enum['name']} {{")
        rust_type = TYPES[enum["type"]][0]
        for value in enum["values"]:
            for line in value["doc"].strip().splitlines():
                out.append(f"    /// {line}" if line else "    ///")
            out.append(
                f"    pub const {value['name'].upper()}: {rust_type} = {value['value']};"
            )
        out.append("}")
        out.append("")

    return "\n".join(out)


# --- Python -----------------------------------------------------------------


def render_python(schema: dict) -> str:
    meta = schema["meta"]
    out = ['"""' + BANNER_LINES[0] + " " + BANNER_LINES[1]]
    out += [
        "",
        f"Contrato DSP↔RCP v{meta['version_major']}.{meta['version_minor']} — lado RCP y",
        "banco de pruebas. Es una de las tres implementaciones generadas de la misma",
        "fuente: si las tres no producen los mismos bytes, el codegen está mal.",
        "",
        "Las cargas útiles de array (los bloques de momento de un moment_ray, la traza",
        "de un spectrum_frame) NO se desempaquetan aquí campo a campo: se mapean con",
        "`numpy.frombuffer(buf, '<f4')`, que da una vista sin copia sobre el búfer",
        "recibido. Desempaquetarlas con `struct` anularía la razón de que el cable",
        "lleve f32 denso.",
        '"""',
        "",
        "from __future__ import annotations",
        "",
        "import struct",
        "from dataclasses import dataclass",
        "",
        f"MAGIC = 0x{meta['magic']:08X}",
        f"VERSION_MAJOR = {meta['version_major']}",
        f"VERSION_MINOR = {meta['version_minor']}",
        "",
    ]

    def emit_struct(name: str, fields: list[dict], doc: str) -> None:
        out.append("@dataclass")
        out.append(f"class {to_camel(name)}:")
        out.append(f'    """{doc.strip().splitlines()[0]}"""')
        out.append("")
        out.append(f'    FORMAT = "{struct_format(fields)}"')
        out.append(f"    SIZE = {size_of(fields)}")
        out.append(
            "    FIELDS = (" + ", ".join(f'"{f["name"]}"' for f in fields) + ",)"
        )
        out.append("")
        for field in fields:
            py_type = TYPES[field["type"]][3]
            default = "0.0" if py_type == "float" else "0"
            out.append(f"    {field['name']}: {py_type} = {default}")
        out.append("")
        out.append("    def pack(self) -> bytes:")
        out.append(
            "        return struct.pack(self.FORMAT, "
            "*(getattr(self, name) for name in self.FIELDS))"
        )
        out.append("")
        out.append("    @classmethod")
        out.append(f'    def unpack(cls, data: bytes) -> "{to_camel(name)}":')
        out.append("        return cls(*struct.unpack(cls.FORMAT, data[: cls.SIZE]))")
        out.append("")

    emit_struct("header", schema["header"]["fields"], "Cabecera común a todo mensaje.")

    out.append("class MsgType:")
    out.append('    """Tipos de mensaje que viajan sueltos por el cable."""')
    out.append("")
    for message in wire_messages(schema):
        out.append(f"    {message['name'].upper()} = {message['type_id']}")
    out.append("")

    for message in schema["message"]:
        emit_struct(message["name"], message["fields"], message["doc"])

    for enum in schema["enum"]:
        out.append(f"class {to_camel(enum['name'])}:")
        out.append(f'    """{enum["doc"].strip().splitlines()[0]}"""')
        out.append("")
        for value in enum["values"]:
            out.append(f"    {value['name'].upper()} = {value['value']}")
        out.append("")

    return "\n".join(out)


# --- TypeScript -------------------------------------------------------------


def render_typescript(schema: dict) -> str:
    meta = schema["meta"]
    out = [f"// {line}" for line in BANNER_LINES]
    out += [
        "//",
        f"// Contrato DSP↔RCP v{meta['version_major']}.{meta['version_minor']} — lado MMI.",
        "//",
        "// Little-endian, empaquetado. Los enteros de 64 bits se exponen como",
        "// bigint: no caben en el double de `number` sin perder enteros a partir",
        "// de 2^53, y un timestamp en nanosegundos los supera de sobra.",
        "",
        "/* eslint-disable */",
        "",
        f"export const MAGIC = 0x{meta['magic']:08X};",
        f"export const VERSION_MAJOR = {meta['version_major']};",
        f"export const VERSION_MINOR = {meta['version_minor']};",
        "",
        "const LE = true;",
        "",
    ]

    def emit_struct(name: str, fields: list[dict], doc: str) -> None:
        camel = to_camel(name)
        out.append("/**")
        for line in doc.strip().splitlines():
            out.append(f" * {line}" if line else " *")
        out.append(" */")
        out.append(f"export interface {camel} {{")
        for field in fields:
            doc_lines = field["doc"].strip().splitlines()
            if len(doc_lines) == 1:
                out.append(f"  /** {doc_lines[0]} */")
            else:
                out.append("  /**")
                out.extend(f"   * {line}" if line else "   *" for line in doc_lines)
                out.append("   */")
            out.append(f"  {to_lower_camel(field['name'])}: {TYPES[field['type']][6]};")
        out.append("}")
        out.append("")
        out.append(f"export const {name.upper()}_SIZE = {size_of(fields)};")
        out.append("")

        # Desplazamientos con nombre: el consumidor que quiera leer un solo campo
        # sin materializar el objeto entero los necesita.
        out.append(f"export const {name.upper()}_OFFSETS = {{")
        for field, offset in offsets_of(fields):
            out.append(f"  {to_lower_camel(field['name'])}: {offset},")
        out.append("} as const;")
        out.append("")

        out.append(
            f"export function decode{camel}(view: DataView, base = 0): {camel} {{"
        )
        out.append("  return {")
        for field, offset in offsets_of(fields):
            getter = TYPES[field["type"]][4]
            width = TYPES[field["type"]][2]
            args = "base + " + str(offset) + (", LE" if width > 1 else "")
            out.append(f"    {to_lower_camel(field['name'])}: view.{getter}({args}),")
        out.append("  };")
        out.append("}")
        out.append("")

        out.append(
            f"export function encode{camel}(value: {camel}, "
            f"view?: DataView, base = 0): DataView {{"
        )
        out.append(
            f"  const dv = view ?? new DataView(new ArrayBuffer({name.upper()}_SIZE));"
        )
        for field, offset in offsets_of(fields):
            setter = TYPES[field["type"]][5]
            width = TYPES[field["type"]][2]
            name_ts = to_lower_camel(field["name"])
            args = f"base + {offset}, value.{name_ts}" + (", LE" if width > 1 else "")
            out.append(f"  dv.{setter}({args});")
        out.append("  return dv;")
        out.append("}")
        out.append("")

    emit_struct("header", schema["header"]["fields"], "Cabecera común a todo mensaje.")

    out.append("/** Tipos de mensaje que viajan sueltos por el cable. */")
    out.append("export const MsgType = {")
    for message in wire_messages(schema):
        out.append(f"  /** {message['dir']} */")
        out.append(f"  {message['name'].upper()}: {message['type_id']},")
    out.append("} as const;")
    out.append("")

    for message in schema["message"]:
        emit_struct(message["name"], message["fields"], message["doc"])

    for enum in schema["enum"]:
        out.append("/**")
        for line in enum["doc"].strip().splitlines():
            out.append(f" * {line}" if line else " *")
        out.append(" */")
        out.append(f"export const {to_camel(enum['name'])} = {{")
        for value in enum["values"]:
            out.append(f"  /** {value['doc']} */")
            out.append(f"  {value['name'].upper()}: {value['value']},")
        out.append("} as const;")
        out.append("")

    return "\n".join(out)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    schema = load_schema()
    outputs = {
        OUT_DIR / "dsp_rcp_v0_1.rs": render_rust(schema),
        OUT_DIR / "dsp_rcp_v0_1.py": render_python(schema),
        OUT_DIR / "dsp_rcp_v0_1.ts": render_typescript(schema),
    }

    if args.check:
        stale = [
            p for p, text in outputs.items() if not p.exists() or p.read_text() != text
        ]
        for path in stale:
            print(f"DESACTUALIZADO: {path.relative_to(ROOT)}", file=sys.stderr)
        if stale:
            print("Ejecuta: python3 tools/gen_contract.py", file=sys.stderr)
            return 1
        print("contrato generado al día")
        return 0

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for path, text in outputs.items():
        path.write_text(text)
        print(f"escrito {path.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
