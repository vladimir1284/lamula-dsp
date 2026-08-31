//! Estimador pulse-pair de momentos del LAMULA DSP
//! (`docs/algorithms/pulse-pair-moments.md`).
//!
//! Cubre la única pieza con oráculo en
//! `tools/oracles/pulse_pair_moments.ipynb`: potencia (`S`), velocidad radial
//! (`V`) y ancho espectral (`W`) a partir de la autocovarianza a retardo 0 y
//! 1 de una ráfaga I/Q. El estimador espectral (FFT + ajuste) queda fuera de
//! este crate — es un modo alternativo distinto, sin oráculo todavía.

mod pulse_pair;

pub use pulse_pair::{pulse_pair_moments, PulsePairEstimate, PulsePairFlag};
