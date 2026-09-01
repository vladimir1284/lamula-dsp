//! Acquisition Abstraction Layer (AAL), pipeline de ingesta y ensamblado de
//! radial del LAMULA DSP (`docs/dsp-plan.md` §4.4; hito M1,
//! `docs/dsp-plan.md:218`: "el simulador emite tramas I/Q sintéticas; la AAL
//! las ingiere; el pipeline produce un rayo de reflectividad sin corregir").
//!
//! Tres adapters detrás del mismo tipo de retorno, [`IngestSource`], en vez
//! de un trait-object async (Rust estable no permite `dyn Trait` con
//! métodos `async fn` sin la dependencia `async-trait`, que este crate evita):
//!
//! - [`simulator`] — en memoria, sin red: reenvía bytes ya generados por
//!   `lamula_simulator::pack_rays`. Sirve para validar la AAL y el pipeline
//!   sin hardware.
//! - [`tcp`] — adapter real: el DSP escucha (servidor), el DRx conecta como
//!   cliente. **Supuesto sin confirmar contra el proyecto LAMULA DRx** — el
//!   contrato `DRx↔DSP` sólo define bytes, no semántica de socket.
//! - [`udp`] — adapter real: un datagrama es una trama completa, sin
//!   garantía de entrega ni de orden (huecos de `seq` los detecta
//!   [`RadialAssembler`], no este módulo), sin filtro de origen (v0.1).
//!
//! El canal bounded de `IngestSource` es el backpressure real:
//! `tx.send(frame).await` espera si el consumidor no drena.
//!
//! Fuera de alcance de este crate (trabajo futuro, ver el plan de ingesta):
//! codificación de salida al RCP (`dsp_rcp::MomentRay`), plano de
//! control/config real (setup vs running, self-test), Status & BITE Manager,
//! archivo de I/Q crudo.
//!
//! [`angle`] convierte `azimuth_raw`/`elevation_raw` (cuenta cruda de encoder
//! SSI) a grados — ver su doc-comment para qué parámetros de ese cálculo
//! siguen sin dueño (resolución y offset de cero del encoder).

mod angle;
mod assembly;
mod error;
pub mod simulator;
pub mod tcp;
pub mod udp;
mod wire;

pub use angle::ssi_counts_to_deg;
pub use assembly::{AssembledRadial, RadialAssembler};
pub use error::IngestError;
pub use wire::{decode_ray_frame, RawPulseFrame};

/// Un adapter en marcha: tramas decodificadas en orden por `frames`, más el
/// `task` en el que corre (para propagar errores o hacer `abort`/`await`).
pub struct IngestSource {
    pub frames: tokio::sync::mpsc::Receiver<RawPulseFrame>,
    pub task: tokio::task::JoinHandle<Result<(), IngestError>>,
}
