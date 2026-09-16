//! Adapter AAL real sobre UDP: un datagrama es una trama completa (mismo
//! supuesto que hace natural que `lamula_simulator::pack_rays` devuelva
//! `Vec<Vec<u8>>`, una entrada por trama — encaja 1:1 con un `send()` UDP por
//! trama). Sin garantía de entrega ni de orden por parte de UDP: los huecos
//! de `seq` los detecta `crate::assembly::RadialAssembler`, no este módulo.
//! Sin filtro de origen (single-source, v0.1).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::net::{ToSocketAddrs, UdpSocket};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::IngestError;
use crate::wire::decode_ray_frame;
use crate::IngestSource;

/// Tamaño máximo teórico de payload UDP sobre IPv4.
const MAX_DATAGRAM: usize = 65_507;

/// Reserva el socket. Separado de [`spawn`] para que quien llama pueda leer
/// el puerto real cuando `addr` pide el puerto 0 (típico en tests).
pub async fn bind(addr: impl ToSocketAddrs) -> Result<UdpSocket, IngestError> {
    Ok(UdpSocket::bind(addr).await?)
}

/// Decodifica cada datagrama que llegue a `socket`. `full_scale_counts` como
/// en `crate::wire::decode_ray_frame`.
///
/// Un datagrama que `decode_ray_frame` rechaza se cuenta en
/// `IngestSource::malformed_frames` y se descarta, sin terminar la tarea —
/// mismo cambio y mismo motivo que `tcp::spawn` (ver su doc-comment), y aquí
/// más simple todavía: un datagrama UDP ya es una trama completa, así que no
/// hay sincronía de bytes que preservar entre uno y el siguiente.
pub fn spawn(socket: UdpSocket, full_scale_counts: i16, capacity: usize) -> IngestSource {
    let (tx, rx) = mpsc::channel(capacity);
    // `IngestSource::afc` no tiene destino en un datagrama sin conexión de
    // vuelta confirmada (v0.1, sin filtro de origen): se acepta y se
    // descarta, igual que el adapter `simulator`.
    let (afc_tx, _afc_rx) = mpsc::channel(1);
    let malformed_frames = Arc::new(AtomicU64::new(0));
    let task: JoinHandle<Result<(), IngestError>> = {
        let malformed_frames = Arc::clone(&malformed_frames);
        tokio::spawn(async move {
            let mut buf = vec![0u8; MAX_DATAGRAM];
            loop {
                // A diferencia de TCP, un socket UDP no tiene "cierre de
                // conexión": un error de `recv` aquí es un fallo real (socket
                // cerrado por el proceso, error del sistema), no un fin de
                // sesión normal — se propaga, no se traga en silencio.
                let n = socket.recv(&mut buf).await?;
                let frame = match decode_ray_frame(&buf[..n], full_scale_counts) {
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
        })
    };
    IngestSource {
        frames: rx,
        afc: afc_tx,
        task,
        malformed_frames,
    }
}
