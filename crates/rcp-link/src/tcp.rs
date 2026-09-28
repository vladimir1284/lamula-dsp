//! Adapter TCP real del enlace `DSP↔RCP`. **El DSP conecta (cliente) y el
//! RCP escucha (servidor)**, por la regla de despliegue "el productor
//! conecta" que gobierna los dos enlaces de la cadena: el DRx conecta al
//! DSP (`lamula_ingest::tcp`, donde el DSP sí escucha) y el DSP conecta al
//! RCP. En los dos casos el consumidor está siempre levantado y el
//! productor es quien rearranca, así que la reconexión vive en un solo lado
//! de cada enlace.
//!
//! Eso NO contradice "the RCP is the sole client" (`docs/dsp-plan.md:262`):
//! esa frase describe el rol arquitectónico — el RCP es quien manda
//! control/config, consume el flujo de momentos, archiva y alimenta a ORPG,
//! y el DSP no tiene GUI ni habla con ORPG — no quién abre el socket. El
//! RCP sigue siendo el único par de este enlace.
//!
//! El framing es uniforme para los diez tipos de mensaje: 12 B de cabecera
//! común, luego `payload_len` bytes más (ver `crate::wire`). Eso evita el
//! `RAY_SIZE` especial que necesita `lamula_ingest::tcp` para el contrato
//! `DRx↔DSP`, donde `payload_len` no cuenta la cabecera del mensaje.
//!
//! Reconecta: al cerrarse una conexión (cierre limpio del lector, o error de
//! escritura como `BrokenPipe`/`ConnectionReset` porque el RCP ya se fue)
//! vuelve a conectar tras [`RECONNECT_DELAY`] y sigue sirviendo los mismos
//! canales `down`/`up`, en vez de terminar la tarea. Un `connect` que falla
//! tampoco es fatal y se reintenta igual: al arrancar el sistema el DSP
//! puede estar levantado antes que el RCP, y un RCP que se reinicia no debe
//! llevarse por delante al DSP. Lo que sí sigue siendo fatal es un error de
//! **decodificación** (magic, versión, longitud): eso no es transporte, es
//! el par hablando otro idioma, y reconectar sólo lo escondería. El contrato
//! exige un
//! `selftest_request`/`selftest_result` en cada reconexión (ver el esquema:
//! "Obligatorio en cada reconexión del RCP"), pero eso lo inicia el RCP —
//! este módulo sólo responde, no lo fuerza. El `Session` (fase/config) no se
//! resetea al reconectar: no hay nada en el contrato ni en el plan que diga
//! que una reconexión de transporte deba tirar la configuración vigente.
//! Mientras no hay conexión, los `UpMessage` mandados por `up` se acumulan
//! en el canal (sujeto a `up_capacity`) y se drenan al reconectar; no se
//! descartan ni se bloquea a quien los manda salvo que el canal esté lleno.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use lamula_contract::dsp_rcp::HEADER_SIZE;

use crate::error::RcpLinkError;
use crate::wire::{decode_down_frame, encode_up_message, DownMessage, UpMessage};

/// Espera entre intentos de conexión al RCP. Fija, no exponencial: los dos
/// extremos viven en la misma red privada de operación, un reintento por
/// segundo no satura nada, y una espera creciente sólo retrasaría la
/// reconexión justo cuando el operador está esperando a que vuelva el flujo.
pub const RECONNECT_DELAY: Duration = Duration::from_secs(1);

/// Un enlace en marcha: mensajes `down` ya decodificados por `down`, un
/// `up` para mandar mensajes `up`, y `task` para propagar errores o hacer
/// `abort`/`await`. `up` se cierra dejándolo caer; el lector termina solo
/// cuando el RCP cierra la conexión.
pub struct RcpLink {
    pub down: mpsc::Receiver<DownMessage>,
    pub up: mpsc::Sender<UpMessage>,
    pub task: JoinHandle<Result<(), RcpLinkError>>,
}

/// Conecta a `addr` (el RCP), una y otra vez mientras haga falta, y sirve el
/// enlace bidireccional sobre cada conexión: una tarea decodifica cada trama
/// `down` que llegue y la manda por `down`; en paralelo, este bucle codifica
/// cada [`UpMessage`] recibido por `up` y lo escribe al socket.
/// `down_capacity`/`up_capacity` son el backpressure real de cada canal. Ver
/// el doc del módulo para la semántica de reconexión.
/// `header_flags` se estampa en la cabecera de cada mensaje `up`. Es una
/// propiedad del despliegue, no del mensaje: hoy su único bit es
/// `header_flag::SIMULATED_SOURCE`, que declara que los datos vienen de un
/// simulador y no del DRx real. Quien monta el enlace lo sabe; los
/// productores de mensajes, no.
pub fn spawn(
    addr: impl Into<String>,
    down_capacity: usize,
    up_capacity: usize,
    header_flags: u8,
) -> RcpLink {
    let (down_tx, down_rx) = mpsc::channel(down_capacity);
    let (up_tx, mut up_rx) = mpsc::channel::<UpMessage>(up_capacity);
    let addr = addr.into();

    let task: JoinHandle<Result<(), RcpLinkError>> = tokio::spawn(async move {
        loop {
            let socket = match TcpStream::connect(&addr).await {
                Ok(socket) => socket,
                // El RCP todavía no está levantado, o se está reiniciando.
                // No es un fallo de este componente: esperar y reintentar.
                Err(e) => {
                    eprintln!(
                        "RCP en {addr} no acepta conexión ({e}); reintento en {RECONNECT_DELAY:?}"
                    );
                    tokio::time::sleep(RECONNECT_DELAY).await;
                    continue;
                }
            };
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
                            let bytes = encode_up_message(&msg, header_flags);
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
                            tokio::time::sleep(RECONNECT_DELAY).await;
                            continue;
                        }
                        Err(mpsc::error::TryRecvError::Empty) => {
                            tokio::time::sleep(RECONNECT_DELAY).await;
                            continue;
                        }
                    }
                }
                Err(RcpLinkError::LinkClosed) => return Ok(()),
                // Error de transporte: el RCP se cayó o la red se cortó. Con
                // el DSP de lado cliente eso es un evento normal de
                // operación, no un fallo de este componente — se reconecta,
                // igual que tras un cierre limpio.
                Err(RcpLinkError::Io(e)) => {
                    eprintln!(
                        "enlace con el RCP en {addr} caído ({e}); reintento en {RECONNECT_DELAY:?}"
                    );
                    tokio::time::sleep(RECONNECT_DELAY).await;
                    continue;
                }
                // Magic, versión o longitud inválidos: el par no habla este
                // contrato. Reconectar sólo repetiría el error en bucle y lo
                // escondería del operador.
                Err(e) => return Err(e),
            }
        }
    });

    RcpLink {
        down: down_rx,
        up: up_tx,
        task,
    }
}
