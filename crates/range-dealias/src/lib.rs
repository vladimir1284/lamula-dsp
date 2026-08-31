//! Dealiasing de rango del LAMULA DSP
//! (`docs/algorithms/dealiasing-de-rango.md`).
//!
//! Cubre los dos peldaños que la página fija como alcance de Stage 1 y que
//! tienen oráculo en `tools/oracles/dealiasing_de_rango.ipynb`: detección y
//! marcado por comparación dual-PRF (toda instalación) y recuperación del
//! primer trip por fase aleatoria (instalación de magnetrón), reutilizando
//! sin reimplementar la corrección de fase de
//! [`lamula-burst`](../burst) y el estimador pulse-pair de
//! [`lamula-moments`](../moments).
//!
//! Fuera de alcance de este crate (ver la página y el oráculo): SZ(8/64),
//! diferido a Stage 2 en su propia página; la vía indirecta de información
//! de rango por staggered-PRT, que la página menciona sin criterio de
//! aceptación propio; y la interacción con polarimetría alternante, nota de
//! diseño sin cantidad numérica propia en el oráculo. El modelo de
//! probabilidad de detección en función de la SNR que usa el oráculo para
//! ejercitar la Prueba 1 es explícitamente ilustrativo, no parte del
//! algoritmo — este crate implementa sólo la reconciliación dual-PRF en sí.

mod detect;
mod recover;

pub use detect::{classify_trip, TripClassification};
pub use recover::recover_trip1;
