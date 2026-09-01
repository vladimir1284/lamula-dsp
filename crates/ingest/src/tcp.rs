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
//!
//! Reconecta: un cierre limpio del DRx (EOF entre tramas) vuelve a
//! `listener.accept()` y sigue mandando por el mismo canal `frames`, en vez
//! de terminar la tarea. Un error real de socket (reset, EOF a mitad de
//! trama, io error) sí termina la tarea y cierra `frames` — quien llame debe
//! tratar eso como fallo fatal de este componente, no como una desconexión
//! normal. El `RadialAssembler` no se resetea al reconectar: si el DRx corta
//! a mitad de un radial, la próxima trama tras la reconexión se sigue
//! alimentando al ensamblador que ya tenía en curso; esto puede producir un
//! radial corrupto — no hay lógica de resincronización en este workspace.

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

/// Acepta conexiones sobre `listener`, una detrás de otra, y decodifica cada
/// trama que llegue. `full_scale_counts` como en
/// `crate::wire::decode_ray_frame`. Ver el doc del módulo para la semántica
/// de reconexión.
pub fn spawn(listener: TcpListener, full_scale_counts: i16, capacity: usize) -> IngestSource {
    let (tx, rx) = mpsc::channel(capacity);
    let task: JoinHandle<Result<(), IngestError>> = tokio::spawn(async move {
        loop {
            let (mut socket, _peer) = listener.accept().await?;
            loop {
                let mut header = [0u8; HEADER_SIZE];
                match socket.read_exact(&mut header).await {
                    Ok(_) => {}
                    // Cierre limpio del otro lado justo entre tramas: fin de
                    // esta conexión, no un fallo — vuelve a esperar la
                    // próxima. Cualquier otro error (reset, timeout, EOF a
                    // mitad de cabecera) se propaga: no se traga un fallo
                    // real de socket como si fuera un cierre limpio.
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
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
        }
    });
    IngestSource { frames: rx, task }
}
