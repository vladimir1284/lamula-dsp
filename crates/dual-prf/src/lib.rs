//! Desdoblado (dealiasing) de velocidad dual-PRF del LAMULA DSP
//! (`docs/algorithms/dual-prf-dealiasing.md`).
//!
//! Por teorema chino del resto: dos PRFs en razón simple comparten la misma
//! velocidad radial verdadera pero plegada de forma distinta, y comparar las
//! dos medidas de pulse-pair resuelve el múltiplo de plegado. Más corrección
//! por continuidad espacial, que la propia página señala como estructural al
//! método —no un accesorio—: en ciertos puntos de la Nyquist extendida (para
//! razón 2:3, en torno al 88%) dos múltiplos de plegado reconcilian casi
//! igual de bien incluso sin ruido, y sólo la vecindad ya resuelta desambigua.
//!
//! Funciones puras sobre velocidades pulse-pair ya calculadas por
//! [`lamula-moments`](../../moments), sin dependencia de producción propia.
//! Contrastado numéricamente contra `tools/oracles/dual_prf_dealiasing.ipynb`
//! en `crates/dual-prf/tests/against_oracle.rs`.
//!
//! Fuera de alcance: la restricción de configuración con polarimetría
//! alternante (bloques de muestras demasiado cortos al combinar alternancia
//! de PRF y de canal) es una regla de rechazo de `dealias_mask`, no una
//! cantidad con verdad-terreno propia.

mod dealias;

pub use dealias::{continuity_fix, dealias_dual_prf, fold, DualPrfEstimate};
