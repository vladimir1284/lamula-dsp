//! Estimación de KDP del LAMULA DSP (`docs/algorithms/kdp-estimacion.md`).
//!
//! Cubre lo que tiene oráculo en `tools/oracles/kdp_estimacion.ipynb`: el
//! enfoque de Stage 1 recomendado por la página — desdoblado de ΦDP,
//! ajuste de mínimos cuadrados en ventana deslizante y derivada como
//! pendiente/2 (Ryzhkov & Zrnić 1996). La censura previa por ρHV bajo es
//! responsabilidad de quien llama (dependencia dura con
//! [`lamula-polarimetry`](../polarimetry), ver "Fuera de alcance" del
//! oráculo): este crate asume un perfil de ΦDP ya censurado aguas arriba.
//!
//! Fuera de alcance (ver la página del algoritmo): las variantes adaptativa
//! (Wang & Chandrasekar 2009) y variacional/iterativa (Vulpiani et al. 2012;
//! Maesaka et al. 2012), diferidas como mejora posterior a Stage 1.

mod unwrap;
mod window_fit;

pub use unwrap::unwrap_deg;
pub use window_fit::kdp_window_fit;
