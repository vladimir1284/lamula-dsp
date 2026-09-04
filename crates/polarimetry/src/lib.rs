//! Estimador de variables polarimétricas del LAMULA DSP
//! (`docs/algorithms/polarimetria-covarianzas.md`).
//!
//! Cubre lo que tiene oráculo en
//! `tools/oracles/polarimetria_covarianzas.ipynb`: ZDR/ρHV/ΦDP en modo
//! simultáneo (STAR); en modo alternante (Sachidananda & Zrnić 1989),
//! ρHV corregido por decorrelación de retardo medio-PRT y ΦDP corregido por
//! el término de fase Doppler que ese mismo retardo introduce en
//! `arg(R_hv)`; y LDR con saturación por aislamiento de antena.
//!
//! Fuera de alcance de este crate (ver la página del algoritmo y el
//! oráculo): el acoplamiento cruzado de modo simultáneo, que la página
//! documenta como límite de modo sin modelo de referencia.

mod covariance;
mod ldr;

pub use covariance::{
    polarimetric_moments_alternating, polarimetric_moments_simultaneous, PolarimetricEstimate,
    PolarimetricFlag,
};
pub use ldr::{ldr_db, LdrEstimate};
