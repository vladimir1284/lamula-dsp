//! Analizador de espectro de FI del LAMULA DSP
//! (`docs/algorithms/analizador-espectro-fi.md`).
//!
//! Cubre lo que tiene oráculo en
//! `tools/oracles/analizador_espectro_fi.ipynb`: periodograma de Welch —
//! ventana, FFT, potencia promediada en lineal (nunca en dB, que sesga a la
//! media geométrica) — con normalización de **ganancia coherente**
//! (`(Σw)²`), correcta para leer el nivel de un tono, y el ENBW (Harris
//! 1978) para corregir la lectura de un suelo de ruido bajo esa misma
//! normalización.
//!
//! Fuera de alcance de este crate (ver la página): la selección de canal,
//! span y frecuencia central a partir de la sintonía del NCO del DRx — es
//! mapeo de configuración, no un algoritmo — y la decisión de arquitectura
//! entre captura oportunista sobre el flujo vivo o un modo dedicado.

mod welch;
mod window;

pub use welch::welch_trace_dbm;
pub use window::{enbw_bins, hann_window};
