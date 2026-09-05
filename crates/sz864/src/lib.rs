//! Recuperación de segundo trip por codificación de fase SZ(8/64) del
//! LAMULA DSP (`docs/algorithms/sz-second-trip-recovery.md`).
//!
//! La página difiere este algoritmo a Stage 2 por decisión de scheduling —
//! no de capacidad de hardware: confirmado 2026-09-04, con transmisor
//! klistrón el excitador sí soporta la modulación de fase programable pulso
//! a pulso que SZ(8/64) exige (la fase se sintetiza digitalmente en FI antes
//! de subir a microondas, sin restricción propia de la etapa de RF). Con
//! magnetrón sigue sin aplicar — la vía ahí es la recuperación por fase
//! aleatoria de [`lamula_range_dealias`](../../range-dealias).
//!
//! Cubre la construcción del código ([`sz_8_64_phases`]) y la separación de
//! dos trips superpuestos ([`separate_trips`]), contrastadas numéricamente
//! contra `tools/oracles/sz_second_trip_recovery.ipynb` en
//! `crates/sz864/tests/against_oracle.rs`. Reutiliza sin reimplementar
//! [`lamula_burst::correct_phase`], [`lamula_moments::pulse_pair_moments`],
//! [`lamula_dual_prf::fold`] y [`lamula_spectral::bin_velocity`].
//!
//! Fuera de alcance (ver la página y el oráculo): ancho espectral de
//! cualquiera de los dos trips (exige "magnitude deconvolution",
//! Sachidananda & Zrnić 1999 / Frush & Doviak 2002); potencia del trip
//! fuerte vía notch con factor de corrección (8/7, 4/3, 2 o 4 según ancho de
//! notch); más de dos trips solapados (la literatura extiende la propiedad
//! de réplicas hasta el 8vo); ventaneo/corrección de sidelobes antes de la
//! FFT del notch (se usa FFT rectangular, notch exacto e invertible); y la
//! interacción con polarimetría alternante (mitad de muestras por canal,
//! nota de diseño de la propia página).

mod code;
mod separate;

pub use code::{sz_8_64_phases, CODE_PERIOD};
pub use separate::{separate_trips, TripSeparationEstimate};
