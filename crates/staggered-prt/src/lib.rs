//! Estimador de velocidad y desdoblado por muestreo escalonado
//! (staggered-PRT) del LAMULA DSP (`docs/algorithms/staggered-prt.md`).
//!
//! A diferencia del [dual-PRF](../../dual-prf), que alterna el PRF entre
//! radiales o bloques de pulsos, staggered-PRT alterna el periodo entre
//! pulsos consecutivos (`T1, T2, T1, T2, …`) *dentro* del mismo radial: las
//! dos autocovarianzas —a retardo `T1` y a retardo `T2`— vienen de la misma
//! realización, sin la brecha temporal que hace al dual-PRF vulnerable a la
//! cizalladura. El desdoblado reutiliza sin reimplementar el mismo mecanismo
//! de reconciliación por teorema chino del resto de `lamula-dual-prf` —la
//! propia página lo señala como "exactamente el mismo mecanismo"—; este
//! crate sólo aporta el estimador de velocidad específico del muestreo
//! escalonado.
//!
//! Contrastado numéricamente contra `tools/oracles/staggered_prt.ipynb` en
//! `crates/staggered-prt/tests/against_oracle.rs`.
//!
//! Filtrado de clutter en muestreo escalonado (Sachidananda & Zrnić 2000):
//! ver [`clutter`], contrastado contra
//! `tools/oracles/staggered_prt_clutter_sz2000.ipynb`. Alcance de Stage 1:
//! notch por subsecuencia, no la reconstrucción gaussiana de
//! `crates/clutter` — el propio oráculo mide y declara esa limitación.
//!
//! Fuera de alcance de este crate: la interacción con polarimetría
//! alternante.

mod clutter;
mod moments;

pub use clutter::{reflectivity_estimate, sz2000_clutter_filter, StaggeredClutterFilter};
pub use moments::staggered_pulse_pair_velocities;
