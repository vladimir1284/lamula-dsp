//! Simulador de I/Q del LAMULA DSP.
//!
//! Implementa el método de Zrnić (1975) para una celda de rango, con
//! espectro Doppler gaussiano y verdad-terreno conocida, más ruido térmico
//! aditivo, empaquetado en tramas `Ray` del contrato `DRx↔DSP`. Canal único
//! y dos canales simultáneos (STAR: ZDR/ΦDP/ρHV, sin LDR).
//!
//! `burst`: secuencias de fase de transmisor (magnetrón aleatorio, deriva de
//! frecuencia con perfil conocido) y la muestra de burst que las porta.
//! `fault`: inyección de fallos sobre tramas ya empaquetadas (tramas
//! malformadas, rayos perdidos, glitches de encoder) — `docs/dsp-plan.md`
//! §3.1 exige las cuatro categorías (más deriva de frecuencia, cubierta por
//! `burst`) como alcance Stage 1 del simulador, para pruebas de BITE.
//!
//! Fuera de alcance de este crate por ahora (ver `docs/algorithms/roadmap.md`
//! y `docs/algorithms/simulador-iq.md`): clutter, ecos de multi-trip, RFI de
//! banda estrecha, y polarización alternante (H/V intercalados en el tiempo,
//! da LDR). Se añaden cuando las fases 2/3 del roadmap los necesiten.

mod burst;
mod fault;
mod generate;
mod ray;
mod spectrum;

pub use burst::{
    apply_transmit_phase, drifting_phase_sequence, generate_burst, magnetron_phase_sequence,
};
pub use fault::{corrupt_frame, drop_rays, inject_encoder_glitch, FrameFault};
pub use generate::{generate_cell, generate_dual_pol_cell, CellParams, DualPolParams};
pub use ray::{pack_rays, RayHeaderFields};
pub use spectrum::gaussian_doppler_spectrum;
