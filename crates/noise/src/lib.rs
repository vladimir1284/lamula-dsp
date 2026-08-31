//! Ruido, resta de ruido y censura por `sig_threshold` del LAMULA DSP.
//!
//! Implementa la mitad de `docs/algorithms/ruido-y-umbrales.md` que tiene
//! oráculo en `tools/oracles/ruido_y_umbrales.ipynb`: estimación objetiva del
//! suelo de ruido por Hildebrand & Sekhon (1974) sobre el periodograma de una
//! ráfaga, resta en lineal con recorte a cero, y censura por `sig_threshold`.
//!
//! Fuera de alcance de este crate (ver la página del algoritmo): medida
//! directa en intervalo pasivo y estimación por radial de Ivić et al. (2013)
//! — ninguna de las dos tiene oráculo todavía —, y censura por
//! `log_threshold`/`sqi_threshold`/`ccor_threshold`, que dependen de
//! cantidades que calculan otros algoritmos del pipeline.

mod hs74;
mod periodogram;
mod threshold;

pub use hs74::{hildebrand_sekhon, noise_floor_estimate};
pub use periodogram::{periodogram, total_power};
pub use threshold::{censored_by_sig_threshold, snr_db, subtract_noise};
