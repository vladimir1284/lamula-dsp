//! Adapter AAL real sobre TCP.
//!
//! El DRx escucha (servidor, puerto `lamula_contract::drx_dsp::TCP_PORT`) y el
//! DSP conecta como cliente: es [D-16] del proyecto DRx, medido en placa en su
//! fase Z4.6 (una sola conexión para los dos sentidos). Esto invierte la regla
//! "el productor conecta" con la que este adapter nació, y el motivo está en
//! D-16: el receptor no tiene que conocer la dirección del DSP, y el DSP
//! puede reiniciarse y reconectar sin tocar la placa. El contrato `DRx↔DSP`
//! sólo define bytes, no semántica de socket, así que la regla vive en el
//! despliegue y no en el esquema. `DSP↔RCP` no cambia.
//!
//! [D-16]: https://lamula-drx-docs.pages.dev/alcance/decisiones/#d-16
//!
//! El framing usa `Header.payload_len`: se lee la cabecera de 12 B, se
//! lee exactamente `payload_len` bytes más (el struct `ray` y su carga: el
//! esquema cuenta todo lo que sigue a la cabecera) antes de decodificar — necesario porque TCP no preserva
//! límites de mensaje.
//!
//! Reconecta: el DSP es el cliente, así que cualquier fin de conexión —cierre
//! limpio, reset, EOF a mitad de trama, reinicio de la placa— es un evento
//! normal de operación y no un fallo de este componente: se espera
//! [`RECONNECT_DELAY`] y se vuelve a conectar, igual que hace `lamula_rcp_link`
//! hacia el RCP. Un `connect` que falla (DRx todavía arrancando) también
//! reintenta. La tarea sólo termina cuando el consumidor suelta `frames`. El
//! `RadialAssembler` no se resetea al reconectar: si el DRx corta a mitad de
//! un radial, la próxima trama tras la reconexión se sigue alimentando al
//! ensamblador que ya tenía en curso; esto puede producir un radial corrupto —
//! no hay lógica de resincronización en este workspace.
//!
//! Una trama cuyos bytes se leen completos pero que `decode_ray_frame`
//! rechaza (`BadMagic`/`UnsupportedVersion`/`UnexpectedMsgType`/`Truncated`)
//! ya NO termina la tarea (antes de este cambio sí, vía `?` — un único
//! `BadMagic` tumbaba el enlace entero, lo que hacía imposible ejercitar
//! `docs/dsp-plan.md` §"fault injection ... for BITE testing" de punta a
//! punta sin reconectar a mano). Se cuenta en `malformed_frames` y se sigue
//! leyendo la próxima cabecera: el `read_exact` de arriba ya consumió del
//! socket exactamente `HEADER_SIZE + payload_len` bytes según el
//! propio encabezado de esa trama, así que la sincronía de bytes para la
//! siguiente trama no se pierde — salvo que la propia corrupción haya caído
//! sobre `payload_len` (`FrameFault::PayloadLenTooLarge` en
//! `lamula_simulator::fault`), caso que este adapter no intenta resincronizar
//! (exigiría buscar `MAGIC` byte a byte en el flujo) y que sigue sin cubrir.
//!
//! **Camino de escritura (`Afc`, sentido `down`).** El socket conectado se
//! parte en mitad de lectura y mitad de escritura (`TcpStream::into_split`):
//! la mitad de lectura sigue el bucle de arriba; la mitad de escritura la
//! posee una tarea aparte que drena `IngestSource::afc` y le escribe
//! [`crate::wire::encode_afc_frame`] tal cual llega. Las dos tareas comparten
//! cuál es la mitad de escritura *vigente* a través de un
//! `Arc<Mutex<Option<OwnedWriteHalf>>>` actualizado en cada conexión (y
//! puesto a `None` al reconectar) porque el ciclo de vida de la escritura no
//! sigue al bucle de lectura: una corrección de AFC puede necesitar salir
//! aunque no haya llegado ningún `Ray` nuevo entretanto (el lazo de AFC
//! corre a la cadencia de rayo, no a la de esta tarea). Sin conexión
//! establecida todavía, o tras un error de escritura, la corrección en curso se
//! descarta — no hay cola de reintento; la próxima actualización del lazo de
//! AFC (`docs/algorithms/burst-fase-afc.md` §"Lazo de AFC") la reemplaza.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use lamula_contract::drx_dsp::{Afc, Config, HEADER_SIZE};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::error::IngestError;
use crate::wire::{decode_ray_frame, encode_afc_frame, encode_config_frame};
use crate::IngestSource;

/// Espera entre intentos de conexión al DRx.
pub const RECONNECT_DELAY: Duration = Duration::from_secs(1);

/// Conecta a `addr` (el DRx), una y otra vez mientras haga falta, decodifica
/// cada trama `Ray` que llegue, y manda hacia el DRx cada `Afc`/`Config` que
/// llegue por `IngestSource::afc`/`IngestSource::drx_config`.
/// `full_scale_counts` como en `crate::wire::decode_ray_frame`. Ver el doc del
/// módulo para la semántica de reconexión y de la mitad de escritura.
pub fn spawn(addr: impl Into<String>, full_scale_counts: i16, capacity: usize) -> IngestSource {
    let addr = addr.into();
    let (tx, rx) = mpsc::channel(capacity);
    let (afc_tx, mut afc_rx) = mpsc::channel::<Afc>(capacity);
    let (drx_config_tx, mut drx_config_rx) = mpsc::channel::<Config>(capacity);
    let write_half: Arc<Mutex<Option<OwnedWriteHalf>>> = Arc::new(Mutex::new(None));
    let malformed_frames = Arc::new(AtomicU64::new(0));

    // Tarea de escritura: vive tanto como `afc_tx`/`drx_config_tx` tengan
    // algún emisor vivo (este `spawn`, y quien clone
    // `IngestSource::afc`/`IngestSource::drx_config`), independiente del
    // ciclo accept/reconectar de la tarea de lectura de abajo. Un `select!`
    // entre los dos canales basta: comparten la misma mitad de escritura y
    // nunca hace falta ordenarlos entre sí (no hay ninguna relación de
    // dependencia entre una corrección de AFC y un `Config` nuevo).
    {
        let write_half = Arc::clone(&write_half);
        tokio::spawn(async move {
            // Los `if afc_open`/`if cfg_open` deshabilitan la rama entera
            // cuando su canal ya cerró — sin esto, un `mpsc::Receiver`
            // cerrado devuelve `None` de inmediato en cada `poll` y el
            // `select!` giraría en caliente sobre esa rama en vez de
            // bloquearse en la otra.
            let mut afc_open = true;
            let mut cfg_open = true;
            while afc_open || cfg_open {
                let frame = tokio::select! {
                    afc = afc_rx.recv(), if afc_open => match afc {
                        Some(afc) => encode_afc_frame(&afc),
                        None => { afc_open = false; continue; }
                    },
                    cfg = drx_config_rx.recv(), if cfg_open => match cfg {
                        Some(cfg) => encode_config_frame(&cfg),
                        None => { cfg_open = false; continue; }
                    },
                };
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
                let socket = match TcpStream::connect(&addr).await {
                    Ok(socket) => socket,
                    // El DRx todavía no escucha (arrancando o reiniciándose).
                    Err(e) => {
                        eprintln!(
                            "DRx en {addr} no acepta conexión ({e}); reintento en {RECONNECT_DELAY:?}"
                        );
                        tokio::time::sleep(RECONNECT_DELAY).await;
                        continue;
                    }
                };
                // `afc` y `config` son tramas pequeñas con latencia que
                // importa; Nagle las retendría detrás de ACKs de un flujo de
                // rayos que va en sentido contrario.
                let _ = socket.set_nodelay(true);
                let (mut read_half, w) = socket.into_split();
                *write_half.lock().await = Some(w);
                loop {
                    let mut header = [0u8; HEADER_SIZE];
                    match read_half.read_exact(&mut header).await {
                        Ok(_) => {}
                        // Cualquier fin de conexión —limpio o no— es un evento
                        // normal con el DSP de lado cliente: se reconecta.
                        Err(e) => {
                            if e.kind() != std::io::ErrorKind::UnexpectedEof {
                                eprintln!("enlace con el DRx en {addr} caído ({e})");
                            }
                            break;
                        }
                    }
                    let payload_len =
                        u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;

                    let mut rest = vec![0u8; payload_len];
                    if let Err(e) = read_half.read_exact(&mut rest).await {
                        eprintln!("enlace con el DRx en {addr} caído a mitad de trama ({e})");
                        break;
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
                tokio::time::sleep(RECONNECT_DELAY).await;
            }
        })
    };
    IngestSource {
        frames: rx,
        afc: afc_tx,
        drx_config: drx_config_tx,
        task,
        malformed_frames,
    }
}
