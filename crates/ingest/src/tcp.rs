//! Adapter AAL real sobre TCP.
//!
//! **Supuesto a verificar contra el proyecto LAMULA DRx antes de comisionar
//! contra hardware real:** el DSP escucha (servidor) y el DRx conecta como
//! cliente — el contrato `DRx↔DSP` sólo define bytes, no semántica de
//! socket, y esa decisión vive en el proyecto DRx externo. Si el DRx espera
//! lo contrario, es cambiar esta función, no la arquitectura: el resto del
//! pipeline sólo ve `IngestSource`.
//!
//! El framing usa `Header.payload_len`: se lee la cabecera de 12 B, se
//! calcula el resto de la trama (`RAY_SIZE + payload_len`) y se lee esa
//! cantidad exacta antes de decodificar — necesario porque TCP no preserva
//! límites de mensaje.

use lamula_contract::drx_dsp::{HEADER_SIZE, RAY_SIZE};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::IngestError;
use crate::wire::decode_ray_frame;
use crate::IngestSource;

/// Escucha en `addr`. Separado de [`spawn`] para que quien llama pueda leer
/// el puerto real cuando `addr` pide el puerto 0 (típico en tests).
pub async fn bind(addr: impl ToSocketAddrs) -> Result<TcpListener, IngestError> {
    Ok(TcpListener::bind(addr).await?)
}

/// Acepta una única conexión sobre `listener` y decodifica cada trama que
/// llegue por ella. `full_scale_counts` como en
/// `crate::wire::decode_ray_frame`.
pub fn spawn(listener: TcpListener, full_scale_counts: i16, capacity: usize) -> IngestSource {
    let (tx, rx) = mpsc::channel(capacity);
    let task: JoinHandle<Result<(), IngestError>> = tokio::spawn(async move {
        let (mut socket, _peer) = listener.accept().await?;
        loop {
            let mut header = [0u8; HEADER_SIZE];
            match socket.read_exact(&mut header).await {
                Ok(_) => {}
                // Cierre limpio del otro lado justo entre tramas: fin de
                // sesión normal, no un fallo. Cualquier otro error (reset,
                // timeout, EOF a mitad de cabecera) se propaga: no se traga
                // un fallo real de socket como si fuera un cierre limpio.
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
                Err(e) => return Err(e.into()),
            }
            let payload_len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;

            let mut rest = vec![0u8; RAY_SIZE + payload_len];
            socket.read_exact(&mut rest).await?;

            let mut full_frame = Vec::with_capacity(HEADER_SIZE + rest.len());
            full_frame.extend_from_slice(&header);
            full_frame.extend_from_slice(&rest);

            let frame = decode_ray_frame(&full_frame, full_scale_counts)?;
            if tx.send(frame).await.is_err() {
                return Ok(());
            }
        }
    });
    IngestSource { frames: rx, task }
}
