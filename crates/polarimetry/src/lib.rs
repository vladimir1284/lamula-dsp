//! Estimador de variables polarimétricas del LAMULA DSP
//! (`docs/algorithms/polarimetria-covarianzas.md`).
//!
//! Cubre lo que tiene oráculo en
//! `tools/oracles/polarimetria_covarianzas.ipynb`: ZDR/ρHV/ΦDP en modo
//! simultáneo (STAR), la corrección de decorrelación de retardo medio-PRT en
//! modo alternante (Sachidananda & Zrnić 1989) para ρHV, y LDR con
//! saturación por aislamiento de antena.
//!
//! Fuera de alcance de este crate (ver la página del algoritmo y el
//! oráculo): la corrección de ΦDP en modo alternante por el término de fase
//! Doppler introducido por el retardo medio-PRT — el oráculo no la valida
//! todavía, así que `phidp_deg` en modo alternante se calcula con la misma
//! fórmula directa que en modo simultáneo y conserva ese sesgo sin corregir;
//! y el acoplamiento cruzado de modo simultáneo, que la página documenta como
//! límite de modo sin modelo de referencia.

mod covariance;
mod ldr;

pub use covariance::{
    polarimetric_moments_alternating, polarimetric_moments_simultaneous, PolarimetricEstimate,
    PolarimetricFlag,
};
pub use ldr::{ldr_db, LdrEstimate};
