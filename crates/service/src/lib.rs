//! Biblioteca del proceso DSP real: `ray` (construcción de `MomentRay`/
//! `SpectrumFrame` a partir de un radial ensamblado) y `config` (parámetros
//! de arranque por variable de entorno). El binario `lamula-dsp`
//! (`src/main.rs`) depende de este crate en vez de declarar sus propios
//! `mod`, precisamente para que `benches/moment_ray.rs` (Criterion) pueda
//! depender de lo mismo — un target `[[bench]]` sólo puede enlazar contra un
//! target de biblioteca, nunca contra `src/main.rs` (que además define `fn
//! main`, incompatible con ser dependencia).

pub mod config;
pub mod ray;
