//! Burst de transmisión, corrección de fase y AFC del LAMULA DSP
//! (`docs/algorithms/burst-fase-afc.md`).
//!
//! Cubre la parte de la página con oráculo en
//! `tools/oracles/burst_fase_afc.ipynb`: medida de fase y frecuencia del
//! burst, corrección de fase coherent-on-receive, y el lazo de AFC de primer
//! orden con congelamiento y BITE ante pérdida de burst. Fuera de alcance:
//! la medida de amplitud del burst como entrada al BITE de potencia
//! transmitida (no tiene oráculo propio todavía) y los límites de excursión
//! máxima / velocidad de cambio del lazo, que son configuración de
//! instalación, no algoritmo.

mod afc;
mod phase;

pub use afc::{loop_gain, AfcLoop, AfcUpdate};
pub use phase::{burst_freq_estimate, burst_phase_estimate, correct_phase};
