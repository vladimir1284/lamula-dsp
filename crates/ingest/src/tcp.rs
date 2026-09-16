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
//!
//! Una trama cuyos bytes se leen completos pero que `decode_ray_frame`
//! rechaza (`BadMagic`/`UnsupportedVersion`/`UnexpectedMsgType`/`Truncated`)
//! ya NO termina la tarea (antes de este cambio sí, vía `?` — un único
//! `BadMagic` tumbaba el enlace entero, lo que hacía imposible ejercitar
//! `docs/dsp-plan.md` §"fault injection ... for BITE testing" de punta a
//! punta sin reconectar a mano). Se cuenta en `malformed_frames` y se sigue
//! leyendo la próxima cabecera: el `read_exact` de arriba ya consumió del
//! socket exactamente `HEADER_SIZE + RAY_SIZE + payload_len` bytes según el
//! propio encabezado de esa trama, así que la sincronía de bytes para la
//! siguiente trama no se pierde — salvo que la propia corrupción haya caído
//! sobre `payload_len` (`FrameFault::PayloadLenTooLarge` en
//! `lamula_simulator::fault`), caso que este adapter no intenta resincronizar
//! (exigiría buscar `MAGIC` byte a byte en el flujo) y que sigue sin cubrir.
//!
//! **Camino de escritura (`Afc`, sentido `down`).** El socket aceptado se
//! parte en mitad de lectura y mitad de escritura (`TcpStream::into_split`):
//! la mitad de lectura sigue el bucle de arriba; la mitad de escritura la
//! posee una tarea aparte que drena `IngestSource::afc` y le escribe
//! [`crate::wire::encode_afc_frame`] tal cual llega. Las dos tareas comparten
//! cuál es la mitad de escritura *vigente* a través de un
//! `Arc<Mutex<Option<OwnedWriteHalf>>>` actualizado en cada `accept()` (y
//! puesto a `None` al reconectar) porque el ciclo de vida de la escritura no
//! sigue al bucle de lectura: una corrección de AFC puede necesitar salir
//! aunque no haya llegado ningún `Ray` nuevo entretanto (el lazo de AFC
//! corre a la cadencia de rayo, no a la de esta tarea). Sin conexión
//! aceptada todavía, o tras un error de escritura, la corrección en curso se
//! descarta — no hay cola de reintento; la próxima actualización del lazo de
//! AFC (`docs/algorithms/burst-fase-afc.md` §"Lazo de AFC") la reemplaza.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use lamula_contract::drx_dsp::{Afc, HEADER_SIZE, RAY_SIZE};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::error::IngestError;
use crate::wire::{decode_ray_frame, encode_afc_frame};
use crate::IngestSource;

/// Escucha en `addr`. Separado de [`spawn`] para que quien llama pueda leer
/// el puerto real cuando `addr` pide el puerto 0 (típico en tests).
pub async fn bind(addr: impl ToSocketAddrs) -> Result<TcpListener, IngestError> {
    Ok(TcpListener::bind(addr).await?)
}

/// Acepta conexiones sobre `listener`, una detrás de otra, decodifica cada
/// trama `Ray` que llegue, y manda hacia el DRx cada `Afc` que llegue por
/// `IngestSource::afc`. `full_scale_counts` como en
/// `crate::wire::decode_ray_frame`. Ver el doc del módulo para la semántica
/// de reconexión y de la mitad de escritura.
pub fn spawn(listener: TcpListener, full_scale_counts: i16, capacity: usize) -> IngestSource {
    let (tx, rx) = mpsc::channel(capacity);
    let (afc_tx, mut afc_rx) = mpsc::channel::<Afc>(capacity);
    let write_half: Arc<Mutex<Option<OwnedWriteHalf>>> = Arc::new(Mutex::new(None));
    let malformed_frames = Arc::new(AtomicU64::new(0));

    // Tarea de escritura: vive tanto como `afc_tx` tenga algún emisor vivo
    // (este `spawn`, y quien clone `IngestSource::afc`), independiente del
    // ciclo accept/reconectar de la tarea de lectura de abajo.
    {
        let write_half = Arc::clone(&write_half);
        tokio::spawn(async move {
            while let Some(afc) = afc_rx.recv().await {
                let frame = encode_afc_frame(&afc);
                let mut guard = write_half.lock().await;
                if let Some(w) = guard.as_mut() {
                    if w.write_all(&frame).await.is_err() {
                        // Escritura rota: la lectura del mismo socket lo
                        // detectará por su cuenta (EOF/error) y reconectará;
                        // aquí sólo se deja de intentar escribir a un socket
                        // muerto hasta la próxima conexión.
                        *guard = None;
                    }
                }
                // Sin conexión aceptada todavía: se descarta en silencio,
                // ver el doc del módulo.
            }
        });
    }

    let task: JoinHandle<Result<(), IngestError>> = {
        let malformed_frames = Arc::clone(&malformed_frames);
        tokio::spawn(async move {
            loop {
                let (socket, _peer) = listener.accept().await?;
                let (mut read_half, w) = socket.into_split();
                *write_half.lock().await = Some(w);
                loop {
                    let mut header = [0u8; HEADER_SIZE];
                    match read_half.read_exact(&mut header).await {
                        Ok(_) => {}
                        // Cierre limpio del otro lado justo entre tramas: fin
                        // de esta conexión, no un fallo — vuelve a esperar la
                        // próxima. Cualquier otro error (reset, timeout, EOF a
                        // mitad de cabecera) se propaga: no se traga un fallo
                        // real de socket como si fuera un cierre limpio.
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                        Err(e) => {
                            *write_half.lock().await = None;
                            return Err(e.into());
                        }
                    }
                    let payload_len =
                        u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;

                    let mut rest = vec![0u8; RAY_SIZE + payload_len];
                    if let Err(e) = read_half.read_exact(&mut rest).await {
                        *write_half.lock().await = None;
                        return Err(e.into());
                    }

                    let mut full_frame = Vec::with_capacity(HEADER_SIZE + rest.len());
                    full_frame.extend_from_slice(&header);
                    full_frame.extend_from_slice(&rest);

                    let frame = match decode_ray_frame(&full_frame, full_scale_counts) {
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
                *write_half.lock().await = None;
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
