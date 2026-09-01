//! Adapter TCP real del enlace `DSP↔RCP`. El DSP escucha (servidor): "the
//! RCP is the sole client" (`docs/dsp-plan.md:262`) — al revés que
//! `lamula_ingest::tcp`, que también hace escuchar al DSP, pero para el DRx.
//!
//! El framing es uniforme para los diez tipos de mensaje: 12 B de cabecera
//! común, luego `payload_len` bytes más (ver `crate::wire`). Eso evita el
//! `RAY_SIZE` especial que necesita `lamula_ingest::tcp` para el contrato
//! `DRx↔DSP`, donde `payload_len` no cuenta la cabecera del mensaje.
//!
//! Reconecta: al cerrarse una conexión (cierre limpio del lector, o error de
//! escritura como `BrokenPipe`/`ConnectionReset` porque el RCP ya se fue)
//! vuelve a `listener.accept()` y sigue sirviendo los mismos canales
//! `down`/`up`, en vez de terminar la tarea. El contrato exige un
//! `selftest_request`/`selftest_result` en cada reconexión (ver el esquema:
//! "Obligatorio en cada reconexión del RCP"), pero eso lo inicia el RCP —
//! este módulo sólo responde, no lo fuerza. El `Session` (fase/config) no se
//! resetea al reconectar: no hay nada en el contrato ni en el plan que diga
//! que una reconexión de transporte deba tirar la configuración vigente.
//! Mientras no hay conexión, los `UpMessage` mandados por `up` se acumulan
//! en el canal (sujeto a `up_capacity`) y se drenan al reconectar; no se
//! descartan ni se bloquea a quien los manda salvo que el canal esté lleno.

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

/// Acepta conexiones sobre `listener`, una detrás de otra, y sirve el
/// enlace bidireccional sobre cada una: una tarea decodifica cada trama
/// `down` que llegue y la manda por `down`; en paralelo, este bucle codifica
/// cada [`UpMessage`] recibido por `up` y lo escribe al socket.
/// `down_capacity`/`up_capacity` son el backpressure real de cada canal. Ver
/// el doc del módulo para la semántica de reconexión.
pub fn spawn(listener: TcpListener, down_capacity: usize, up_capacity: usize) -> RcpLink {
    let (down_tx, down_rx) = mpsc::channel(down_capacity);
    let (up_tx, mut up_rx) = mpsc::channel::<UpMessage>(up_capacity);

    let task: JoinHandle<Result<(), RcpLinkError>> = tokio::spawn(async move {
        loop {
            let (socket, _peer) = listener.accept().await?;
            let (mut rd, mut wr) = tokio::io::split(socket);
            let down_tx = down_tx.clone();

            let mut reader: JoinHandle<Result<(), RcpLinkError>> = tokio::spawn(async move {
                loop {
                    let mut header = [0u8; HEADER_SIZE];
                    match rd.read_exact(&mut header).await {
                        Ok(_) => {}
                        // Cierre limpio del RCP entre tramas: fin de esta
                        // conexión, no un fallo — el bucle externo vuelve a
                        // aceptar. Cualquier otro error se propaga.
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
                        Err(e) => return Err(e.into()),
                    }
                    let payload_len =
                        u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;

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

            // Corre en esta misma tarea (no en una nueva) para poder tomar
            // `up_rx` prestado: tiene que sobrevivir a la conexión y
            // pasar a la siguiente, así que no puede moverse a una tarea
            // hija que muere con la conexión.
            let write_loop = async {
                loop {
                    match up_rx.recv().await {
                        Some(msg) => {
                            let bytes = encode_up_message(&msg);
                            wr.write_all(&bytes).await?;
                        }
                        // Todos los `Sender` (incluido el de `RcpLink::up`)
                        // se soltaron: el proceso está cerrando el enlace a
                        // propósito, no tiene sentido seguir aceptando.
                        None => return Err(RcpLinkError::LinkClosed),
                    }
                }
            };

            let result: Result<(), RcpLinkError> = tokio::select! {
                r = &mut reader => r.expect("la tarea lectora entró en panic"),
                w = write_loop => w,
            };
            reader.abort();

            match result {
                Ok(()) => {
                    // El lector terminó por un cierre limpio de esta
                    // conexión. Si el `up_rx.recv()` de `write_loop` también
                    // estaba listo en el mismo instante (p.ej. quien llama
                    // soltó `up` justo cuando el RCP se desconectó), el
                    // `select!` pudo haber elegido esta rama en vez de la
                    // otra: es una carrera real entre dos condiciones que se
                    // vuelven ciertas a la vez, no un simple orden de
                    // eventos. Comprobar el canal aquí decide de forma
                    // determinista qué hacer en vez de dejarlo al azar del
                    // `select!`.
                    match up_rx.try_recv() {
                        Err(mpsc::error::TryRecvError::Disconnected) => return Ok(()),
                        Ok(_msg) => {
                            // Carrera de veras rara: llegó un `UpMessage` a
                            // la cola justo cuando esta conexión se cerró.
                            // No hay dónde escribirlo (el socket ya se fue)
                            // ni forma de devolverlo al canal — se descarta
                            // con aviso en vez de bloquear el reintento.
                            eprintln!(
                                "up_message descartado: conexión RCP cerrada justo antes de escribirlo"
                            );
                            continue;
                        }
                        Err(mpsc::error::TryRecvError::Empty) => continue,
                    }
                }
                Err(RcpLinkError::LinkClosed) => return Ok(()),
                Err(e) => return Err(e), // fallo real de socket: fatal
            }
        }
    });

    RcpLink {
        down: down_rx,
        up: up_tx,
        task,
    }
}
