// Auxiliar del test de acuerdo entre lenguajes. No es código de producción.
//
// Lee por stdin un JSON {estructura: {campo: valor}}, codifica cada estructura
// con las funciones generadas de TypeScript y devuelve por stdout el hexadecimal
// de cada una. El test de Python compara esos bytes con los suyos.
//
// Los enteros de 64 bits llegan como cadena y se convierten a BigInt: JSON no
// tiene un entero que llegue a 2^64, y un timestamp en nanosegundos lo pasa.

import { readFileSync } from "node:fs";

const contract = await import("../generated/dsp_rcp_v0_1.ts");

const input = JSON.parse(readFileSync(0, "utf8"));
const output = {};

for (const [structName, fields] of Object.entries(input.structs)) {
  const encode = contract[`encode${structName}`];
  if (typeof encode !== "function") {
    throw new Error(`el módulo generado no exporta encode${structName}`);
  }

  const value = {};
  for (const [field, raw] of Object.entries(fields)) {
    value[field] = typeof raw === "string" ? BigInt(raw) : raw;
  }

  const view = encode(value);
  const bytes = new Uint8Array(
    view.buffer,
    view.byteOffset,
    view.byteLength,
  );
  output[structName] = Buffer.from(bytes).toString("hex");
}

// Los tamaños y desplazamientos exportados viajan también, para que el test
// pueda contrastarlos sin volver a parsear el TypeScript.
output.__sizes = Object.fromEntries(
  Object.keys(input.structs).map((name) => [
    name,
    contract[`${input.constNames[name]}_SIZE`],
  ]),
);

process.stdout.write(JSON.stringify(output));
