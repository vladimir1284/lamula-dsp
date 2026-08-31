//! Estimador espectral de momentos del LAMULA DSP
//! (`docs/algorithms/estimador-espectral.md`).
//!
//! Periodograma con ventana de Hann, umbral de ruido HS74 sobre la serie sin
//! ventanear, recorte de la línea principal por caída relativa desde el pico
//! y recentrado circular del eje de velocidad. Contrastado numéricamente
//! contra `tools/oracles/estimador_espectral.ipynb` en
//! `crates/spectral/tests/against_oracle.rs`.
//!
//! El oráculo deja constancia de un hallazgo que esta implementación hereda
//! sin disimular: en escenarios de un solo modo la varianza de velocidad de
//! este estimador es peor que la del [pulse-pair](../../moments), porque un
//! periodograma de una sola ráfaga no promedia ni ajusta un modelo no
//! lineal. Donde sí gana con claridad es en escenarios bimodales, que es la
//! razón de ser de este modo alternativo.

mod moments;

pub use moments::{spectral_moments, SpectralEstimate, SpectralFlag};
