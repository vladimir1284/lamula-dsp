//! Procesamiento de rango y modos de barrido del LAMULA DSP
//! (`docs/algorithms/procesamiento-de-rango.md`).
//!
//! Cubre la parte de la página con oráculo en
//! `tools/oracles/procesamiento_de_rango.ipynb`: asignación de gate de rango,
//! promediado de celda gruesa (el paso final común, una vez se tienen `K`
//! valores a combinar) y composición de split-cut. Fuera de alcance: el
//! ensamblado de radial a partir de cuentas de encoder SSI (no tiene oráculo
//! todavía) y la estimación de velocidad por pulso-pareado que usa el
//! oráculo para demostrar split-cut (pertenece a
//! `docs/algorithms/pulse-pair-moments.md`, no a esta página).

mod averaging;
mod gate;
mod split_cut;

pub use averaging::average_power;
pub use gate::assign_range_gate;
pub use split_cut::{compose_split_cut, SplitCutMoments};
