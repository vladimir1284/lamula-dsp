//! Simulador de I/Q del LAMULA DSP.
//!
//! Implementa el método de Zrnić (1975) para una celda de rango, con
//! espectro Doppler gaussiano y verdad-terreno conocida, más ruido térmico
//! aditivo, empaquetado en tramas `Ray` del contrato `DRx↔DSP`. Canal único
//! y dos canales simultáneos (STAR: ZDR/ΦDP/ρHV, sin LDR).
//!
//! Fuera de alcance de este crate por ahora (ver `docs/algorithms/roadmap.md`
//! y `docs/algorithms/simulador-iq.md`): clutter, ecos de multi-trip, RFI de
//! banda estrecha, firma de transmisor (magnetrón/coherente + burst) y
//! polarización alternante (H/V intercalados en el tiempo, da LDR). Se
//! añaden cuando las fases 2/3 del roadmap los necesiten.

mod generate;
mod ray;
mod spectrum;

pub use generate::{generate_cell, generate_dual_pol_cell, CellParams, DualPolParams};
pub use ray::{pack_rays, RayHeaderFields};
pub use spectrum::gaussian_doppler_spectrum;
