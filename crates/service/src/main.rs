//! Binario del proceso DSP real (`docs/dsp-plan.md` §4). Hasta este crate,
//! `lamula_ingest` y `lamula_rcp_link` sólo se habían probado unidos en un
//! test (`crates/rcp-link/tests/vertical_slice.rs`), nunca en un proceso que
//! efectivamente corra — es el hueco que quedaba tras cerrar el hito M1.
//! Este binario los conecta: escucha al DRx (AAL) y al RCP (control +
//! datos), corre `lamula_rcp_link::Session` sobre los mensajes down, y
//! cuando la sesión está en `running` ensambla cada radial y lo emite como
//! `MomentRay`.
//!
//! Tanto `lamula_ingest::tcp` como `lamula_rcp_link::tcp` reconectan solos:
//! un cierre limpio de cualquiera de los dos lados vuelve a esperar la
//! próxima conexión sin tirar este proceso ni perder `Session` (fase +
//! config vigente). Este binario sólo termina si una de esas dos tareas
//! muere por un fallo real de socket (no una desconexión normal) — en ese
//! caso no tiene sentido seguir, así que aborta la otra tarea y sale.
//!
//! Alcance honesto — lo que este binario deliberadamente NO hace, porque
//! nada en este workspace lo respalda todavía:
//! - Sólo produce UZ (reflectividad sin corregir) y V (velocidad), vía
//!   `lamula_moments::pulse_pair_moments` sobre el canal 0: es el mismo par
//!   que cierra el hito M1 en el vertical slice. `capabilities` sólo
//!   anuncia esos dos momentos y el estimador pulse-pair — cualquier otro
//!   bit de `moment_mask` en un `config` se rechaza como
//!   `moment_unsupported` antes de llegar aquí
//!   (`lamula_rcp_link::validate::validate_config`). Los crates de
//!   polarimetría/KDP/calidad/dealiasing/clutter que sí existen en el
//!   workspace no están conectados a este binario.
//! - No hay barrido: `volume_seq`/`sweep_seq`/`ray_index` quedan a 0, y
//!   `az_end_deg`/`el_end_deg` valen lo mismo que `az_start_deg`/
//!   `el_start_deg` — no hay controlador de antena en este repo.
//! - `SelftestRequest` se responde siempre, en cualquier fase, con éxito:
//!   no hay autotest de enlace real que ejecutar en este workspace. El plan
//!   (§6.1) exige el intercambio en cada reconexión; este binario lo
//!   satisface a nivel de protocolo, no de contenido.
//! - `request_config` no se rechaza, pero el contrato v0.1 no tiene ningún
//!   `MsgType` que devuelva un `Config` (sólo `config_ack`, `status`,
//!   `capabilities`): hueco real del contrato. Se responde sólo con el
//!   `config_ack` genérico de todo `control`.
//! - `Status.capability_flags`/`bite_flags` y los `noise_floor_dbm_*`/
//!   `dc_offset_*` por canal quedan a 0: el esquema no define el bit-layout
//!   de esos dos campos, y no hay Status & BITE Manager ni medición de
//!   continua en este workspace. `Status` sólo se manda al pedirlo con
//!   `request_status`, no periódicamente ni al cambiar de estado (el plan
//!   pide ambas cosas; este binario no tiene todavía temporizador ni
//!   detección de cambios).
//! - Resolución y offset de cero del encoder SSI no están documentados en
//!   ningún sitio del repo (ver `lamula_ingest::angle`): se piden por
//!   variable de entorno, sin valor por defecto inventado.

mod config;
mod ray;

use lamula_contract::dsp_rcp::{
    command, dealias_mode, error as rcp_error, estimator, moment_kind, Capabilities, ConfigAck,
    SelftestResult, Status, VERSION_MAJOR, VERSION_MINOR,
};
use lamula_ingest::RadialAssembler;
use lamula_rcp_link::wire::{DownMessage, UpMessage};
use lamula_rcp_link::Session;
use tokio::sync::mpsc;

use config::ServiceConfig;

#[derive(Default)]
struct Counters {
    rays_in: u32,
    rays_out: u32,
    rays_dropped: u32,
}

#[tokio::main]
async fn main() {
    let cfg = ServiceConfig::from_env().unwrap_or_else(|e| {
        eprintln!("configuración inválida: {e}");
        std::process::exit(1);
    });

    let drx_listener = lamula_ingest::tcp::bind(&cfg.drx_addr)
        .await
        .unwrap_or_else(|e| panic!("no se pudo escuchar DRx en {}: {e}", cfg.drx_addr));
    println!("DRx (AAL) escuchando en {}", cfg.drx_addr);
    let mut ingest = lamula_ingest::tcp::spawn(drx_listener, cfg.full_scale_counts, 16);

    let rcp_listener = lamula_rcp_link::tcp::bind(&cfg.rcp_addr)
        .await
        .unwrap_or_else(|e| panic!("no se pudo escuchar RCP en {}: {e}", cfg.rcp_addr));
    println!("RCP escuchando en {}", cfg.rcp_addr);
    let link = lamula_rcp_link::tcp::spawn(rcp_listener, 16, 16);
    let mut down = link.down;
    let up = link.up;

    let mut session = Session::new(capabilities());
    let mut assembler: Option<RadialAssembler> = None;
    let mut ray_seq: u32 = 0;
    let mut first_ray_after_config = false;
    let mut counters = Counters::default();
    let start = tokio::time::Instant::now();

    loop {
        tokio::select! {
            frame = ingest.frames.recv() => {
                let Some(frame) = frame else {
                    eprintln!("la tarea de ingesta DRx terminó (fallo real, no una desconexión normal)");
                    link.task.abort();
                    break;
                };
                counters.rays_in += 1;
                let Some(a) = assembler.as_mut() else {
                    continue; // no en `running`: se drena sin ensamblar
                };
                match a.feed(frame) {
                    Ok(Some(radial)) => {
                        let cfg_snapshot = session
                            .config()
                            .expect("running implica config aplicado (Session::handle_command)");
                        ray_seq = ray_seq.wrapping_add(1);
                        let msg = ray::build_moment_ray(
                            &radial,
                            cfg_snapshot,
                            ray_seq,
                            first_ray_after_config,
                            cfg.ssi_counts_per_turn,
                            cfg.ssi_zero_offset_deg,
                        );
                        first_ray_after_config = false;
                        counters.rays_out += 1;
                        if up.send(msg).await.is_err() {
                            println!("RCP no admite más momentos (up cerrado)");
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        counters.rays_dropped += 1;
                        eprintln!("radial descartado: {e}");
                    }
                }
            }
            msg = down.recv() => {
                let Some(msg) = msg else {
                    eprintln!("la tarea del enlace RCP terminó (fallo real, no una desconexión normal)");
                    ingest.task.abort();
                    break;
                };
                handle_down_message(
                    msg,
                    &mut session,
                    &up,
                    &mut assembler,
                    &mut first_ray_after_config,
                    &counters,
                    start,
                )
                .await;
            }
        }
    }

    // Cualquiera de las dos puede llegar aquí abortada (la otra terminó
    // primero por un fallo real): `is_cancelled()` distingue eso de un
    // panic genuino dentro de la tarea.
    match ingest.task.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("ingesta terminó con error: {e}"),
        Err(e) if e.is_cancelled() => {}
        Err(e) => panic!("la tarea de ingesta entró en panic: {e}"),
    }
    match link.task.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("enlace RCP terminó con error: {e}"),
        Err(e) if e.is_cancelled() => {}
        Err(e) => panic!("la tarea del enlace RCP entró en panic: {e}"),
    }
}

async fn handle_down_message(
    msg: DownMessage,
    session: &mut Session,
    up: &mpsc::Sender<UpMessage>,
    assembler: &mut Option<RadialAssembler>,
    first_ray_after_config: &mut bool,
    counters: &Counters,
    start: tokio::time::Instant,
) {
    match msg {
        DownMessage::Config(down_config) => {
            let seq = down_config.seq;
            let error = session
                .apply_config(down_config)
                .err()
                .unwrap_or(rcp_error::OK);
            let _ = up
                .send(UpMessage::ConfigAck(ConfigAck {
                    seq,
                    error,
                    pad0: 0,
                    pad1: 0,
                }))
                .await;
        }
        DownMessage::Control(control) => {
            let seq = control.seq;
            let result = session.handle_command(control.command);
            let error = result.err().unwrap_or(rcp_error::OK);
            let _ = up
                .send(UpMessage::ConfigAck(ConfigAck {
                    seq,
                    error,
                    pad0: 0,
                    pad1: 0,
                }))
                .await;
            if result.is_err() {
                return;
            }
            match control.command {
                command::START => {
                    let n_pulses = session
                        .config()
                        .expect("Session::handle_command(START) ya exigió config aplicado")
                        .n_pulses;
                    *assembler = Some(RadialAssembler::new(n_pulses));
                    *first_ray_after_config = true;
                }
                command::STOP | command::ENTER_SETUP => {
                    *assembler = None;
                }
                command::REQUEST_STATUS => {
                    let status = build_status(session, counters, start);
                    let _ = up.send(UpMessage::Status(status)).await;
                }
                command::REQUEST_CAPABILITIES => {
                    let _ = up.send(UpMessage::Capabilities(capabilities())).await;
                }
                _ => {}
            }
        }
        DownMessage::SelftestRequest(req) => {
            let result = SelftestResult {
                seq: req.seq,
                nonce: req.nonce,
                capability_flags: 0,
                error: rcp_error::OK,
                version_major: VERSION_MAJOR,
                version_minor: VERSION_MINOR,
                pad0: 0,
            };
            let _ = up.send(UpMessage::SelftestResult(result)).await;
        }
    }
}

/// Sólo UZ+V, pulse-pair, sin dealiasing: ver el doc-comment de `crate`
/// para por qué. `max_gates`/`max_pulses` son el techo del tipo de cable
/// (`n_gates`/`n_pulses` son `u16`), no un límite de hardware medido —
/// ningún documento del repo da uno real.
fn capabilities() -> Capabilities {
    Capabilities {
        moment_mask: (1 << moment_kind::UZ) | (1 << moment_kind::V),
        dealias_mask: 1 << dealias_mode::NONE,
        estimator_mask: 1 << estimator::PULSE_PAIR,
        max_gates: u16::MAX as u32,
        max_pulses: u16::MAX,
        n_rx_channels: 1,
        pad0: 0,
    }
}

fn build_status(session: &Session, counters: &Counters, start: tokio::time::Instant) -> Status {
    let config = session.config();
    Status {
        uptime_s: start.elapsed().as_secs() as u32,
        phase: session.phase(),
        config_seq: config.map(|c| c.seq).unwrap_or(0),
        rays_in: counters.rays_in,
        rays_out: counters.rays_out,
        rays_dropped: counters.rays_dropped,
        // Periodo mandado se deriva de `prf_hz`; el medido no se instrumenta
        // en este workspace todavía.
        trigger_period_cmd_ns: config
            .map(|c| (1.0e9 / c.prf_hz as f64) as u32)
            .unwrap_or(0),
        noise_floor_dbm_0: config.map(|c| c.noise_floor_dbm).unwrap_or(0.0),
        n_rx_channels: 1,
        // severity/last_error/capability_flags/bite_flags/queue_depth/
        // bins_ok/bins_total/trigger_period_meas_ns/dc_offset_*/
        // noise_floor_dbm_{1,2,3}: sin fuente real en este workspace, ver
        // el doc-comment de `crate`.
        ..Default::default()
    }
}
