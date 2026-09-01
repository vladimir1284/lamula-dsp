//! Adapter AAL en memoria: reenvía tramas ya generadas por
//! `lamula_simulator::pack_rays` sin red, para validar la AAL y el pipeline
//! sin hardware (hito M1, `docs/dsp-plan.md:218`).

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::IngestError;
use crate::wire::{decode_ray_frame, RawPulseFrame};
use crate::IngestSource;

/// Lanza una fuente que decodifica y entrega, en orden, cada trama de
/// `frames` (normalmente la salida de `pack_rays`) por el canal bounded de
/// `IngestSource`. `capacity` es el tamaño de ese canal — el backpressure
/// real: `send` espera si el consumidor no drena.
pub fn spawn(frames: Vec<Vec<u8>>, full_scale_counts: i16, capacity: usize) -> IngestSource {
    let (tx, rx) = mpsc::channel::<RawPulseFrame>(capacity);
    let task: JoinHandle<Result<(), IngestError>> = tokio::spawn(async move {
        for raw in frames {
            let frame = decode_ray_frame(&raw, full_scale_counts)?;
            if tx.send(frame).await.is_err() {
                return Ok(());
            }
        }
        Ok(())
    });
    IngestSource { frames: rx, task }
}
