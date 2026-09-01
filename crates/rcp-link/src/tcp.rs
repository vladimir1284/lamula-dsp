//! Adapter TCP real del enlace `DSP↔RCP`. El DSP escucha (servidor): "the
//! RCP is the sole client" (`docs/dsp-plan.md:262`) — al revés que
//! `lamula_ingest::tcp`, que también hace escuchar al DSP, pero para el DRx.
//! Asume una única conexión, igual que ese adapter.
//!
//! El framing es uniforme para los diez tipos de mensaje: 12 B de cabecera
//! común, luego `payload_len` bytes más (ver `crate::wire`). Eso evita el
//! `RAY_SIZE` especial que necesita `lamula_ingest::tcp` para el contrato
//! `DRx↔DSP`, donde `payload_len` no cuenta la cabecera del mensaje.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use lamula_contract::dsp_rcp::HEADER_SIZE;

use crate::error::RcpLinkError;
use crate::wire::{decode_down_frame, encode_up_message, DownMessage, UpMessage};

/// Escucha en `addr`. Separado de [`spawn`] para que quien llama pueda leer
/// el puerto real cuando `addr` pide el puerto 0 (típico en tests).
pub async fn bind(addr: impl ToSocketAddrs) -> Result<TcpListener, RcpLinkError> {
    Ok(TcpListener::bind(addr).await?)
}

/// Un enlace en marcha: mensajes `down` ya decodificados por `down`, un
/// `up` para mandar mensajes `up`, y `task` para propagar errores o hacer
/// `abort`/`await`. `up` se cierra dejándolo caer; el lector termina solo
/// cuando el RCP cierra la conexión.
pub struct RcpLink {
    pub down: mpsc::Receiver<DownMessage>,
    pub up: mpsc::Sender<UpMessage>,
    pub task: JoinHandle<Result<(), RcpLinkError>>,
}

/// Acepta una única conexión sobre `listener` y sirve el enlace bidireccional
/// sobre ella: una tarea decodifica cada trama `down` que llegue y la manda
/// por `down`; otra codifica cada [`UpMessage`] recibido por `up` y lo
/// escribe al socket. `down_capacity`/`up_capacity` son el backpressure real
/// de cada canal.
pub fn spawn(listener: TcpListener, down_capacity: usize, up_capacity: usize) -> RcpLink {
    let (down_tx, down_rx) = mpsc::channel(down_capacity);
    let (up_tx, up_rx) = mpsc::channel::<UpMessage>(up_capacity);

    let task: JoinHandle<Result<(), RcpLinkError>> = tokio::spawn(async move {
        let (socket, _peer) = listener.accept().await?;
        let (mut rd, mut wr) = tokio::io::split(socket);

        let reader: JoinHandle<Result<(), RcpLinkError>> = tokio::spawn(async move {
            loop {
                let mut header = [0u8; HEADER_SIZE];
                match rd.read_exact(&mut header).await {
                    Ok(_) => {}
                    // Cierre limpio del RCP entre tramas: fin de sesión
                    // normal, no un fallo. Cualquier otro error se propaga.
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
                    Err(e) => return Err(e.into()),
                }
                let payload_len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;

                let mut rest = vec![0u8; payload_len];
                rd.read_exact(&mut rest).await?;

                let mut full_frame = Vec::with_capacity(HEADER_SIZE + payload_len);
                full_frame.extend_from_slice(&header);
                full_frame.extend_from_slice(&rest);

                let msg = decode_down_frame(&full_frame)?;
                if down_tx.send(msg).await.is_err() {
                    return Ok(());
                }
            }
        });

        let writer: JoinHandle<Result<(), RcpLinkError>> = tokio::spawn(async move {
            let mut up_rx = up_rx;
            while let Some(msg) = up_rx.recv().await {
                let bytes = encode_up_message(&msg);
                wr.write_all(&bytes).await?;
            }
            Ok(())
        });

        let (r, w) = tokio::join!(reader, writer);
        r.expect("la tarea lectora entró en panic")?;
        w.expect("la tarea escritora entró en panic")?;
        Ok(())
    });

    RcpLink {
        down: down_rx,
        up: up_tx,
        task,
    }
}
