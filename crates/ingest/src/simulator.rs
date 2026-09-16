//! Adapter AAL en memoria: reenvía tramas ya generadas por
//! `lamula_simulator::pack_rays` sin red, para validar la AAL y el pipeline
//! sin hardware (hito M1, `docs/dsp-plan.md:218`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::IngestError;
use crate::wire::{decode_ray_frame, RawPulseFrame};
use crate::IngestSource;

/// Lanza una fuente que decodifica y entrega, en orden, cada trama de
/// `frames` (normalmente la salida de `pack_rays`, opcionalmente pasada por
/// `lamula_simulator::fault` para ejercitar `docs/dsp-plan.md` §"fault
/// injection ... for BITE testing") por el canal bounded de `IngestSource`.
/// `capacity` es el tamaño de ese canal — el backpressure real: `send`
/// espera si el consumidor no drena.
///
/// Una trama que `decode_ray_frame` rechaza se cuenta en
/// `IngestSource::malformed_frames` y se descarta, sin terminar la tarea —
/// mismo cambio y mismo motivo que `tcp::spawn` (ver su doc-comment): sin
/// esto, un escenario guionado de BITE con una sola trama corrupta a mitad
/// de la lista nunca llegaba a ver las tramas válidas que la seguían.
pub fn spawn(frames: Vec<Vec<u8>>, full_scale_counts: i16, capacity: usize) -> IngestSource {
    let (tx, rx) = mpsc::channel::<RawPulseFrame>(capacity);
    // Sin transporte real detrás: nadie drena este canal, así que un `send`
    // sobre `IngestSource::afc` falla de inmediato en vez de bloquear (ver su
    // doc-comment). Es intencional — este adapter no tiene DRx del otro lado.
    let (afc_tx, _afc_rx) = mpsc::channel(1);
    let malformed_frames = Arc::new(AtomicU64::new(0));
    let task: JoinHandle<Result<(), IngestError>> = {
        let malformed_frames = Arc::clone(&malformed_frames);
        tokio::spawn(async move {
            for raw in frames {
                let frame = match decode_ray_frame(&raw, full_scale_counts) {
                    Ok(f) => f,
                    Err(_) => {
                        malformed_frames.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                };
                if tx.send(frame).await.is_err() {
                    return Ok(());
                }
            }
            Ok(())
        })
    };
    IngestSource {
        frames: rx,
        afc: afc_tx,
        task,
        malformed_frames,
    }
}
