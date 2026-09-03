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
//! - Produce UZ (reflectividad sin corregir), CZ (corregida,
//!   `lamula_calibration::power_to_dbz`), V (velocidad, desdoblada en
//!   dual-PRF/staggered-PRT), SQI y SIG sobre el canal 0 (`crate::ray`,
//!   sobre `lamula_moments::pulse_pair_moments` y `lamula_quality`), más
//!   ZDR/ρHV/ΦDP/KDP (`lamula_polarimetry` en modo simultáneo/STAR,
//!   `lamula_kdp`) cuando el radial trae un segundo canal. `capabilities`
//!   sólo anuncia esos nueve momentos, el estimador pulse-pair y los modos de
//!   dealiasing dual-PRF y staggered-PRT — cualquier otro bit de `moment_mask` o
//!   `dealias_mode` en un `config` se rechaza como `moment_unsupported`/
//!   `dealias_unsupported` antes de llegar aquí
//!   (`lamula_rcp_link::validate::validate_config`). No hay CCOR porque no
//!   hay filtro de clutter conectado a este binario (el crate
//!   `lamula-clutter` existe en el workspace, pero no está wireado aquí).
//!   Dealiasing de rango (`config.range_dealias`) sólo tiene conectado el
//!   nivel "detección y marcado" (`crate::ray`, cross-radial vía
//!   `PreviousPrf`, inferencia sin respaldo de oráculo — ver su
//!   doc-comment); NO usa `classify_trip` de `lamula-range-dealias`, que
//!   modela un blanco puntual con detección de picos que este pipeline de
//!   eco distribuido no tiene. La recuperación por fase aleatoria
//!   (`recover_trip1`, sólo instalaciones de magnetrón) sigue sin conectar
//!   aquí: ya existe fase de burst por pulso en el wire (`channel::
//!   TX_BURST_0`, `contract/schema/drx_dsp_v0_1.toml` v0.3) y
//!   `crate::ray::burst_phase_correct` la usa para coherent-on-receive, pero
//!   nadie llama a `recover_trip1` con ella todavía — trabajo aparte, no
//!   bloqueado por contrato como antes.
//!   Coherent-on-receive (`crate::ray::burst_phase_correct`) sí está
//!   conectado: corrige la fase de todo canal que no sea `TX_BURST_0` con
//!   `lamula_burst::{burst_phase_estimate, correct_phase}` cuando
//!   `config.burst_window_bins > 0` y el radial trae ese canal — prerrequisito
//!   duro de cualquier estimador Doppler en magnetrón
//!   (`docs/algorithms/burst-fase-afc.md`). El lazo de AFC
//!   (`lamula_burst::AfcLoop`) NO está conectado: exigiría mandar el mensaje
//!   `Afc` (`nco_phase_inc`) de vuelta al DRx, y `lamula_ingest` sólo tiene
//!   camino de lectura sobre esa conexión hoy, no de escritura.
//! - Censura por `sig_threshold`/`sqi_threshold`/`log_threshold`, y por
//!   separado la de ZDR/ρHV/ΦDP/KDP: ver el doc-comment de `crate::ray`.
//!   `ccor_threshold` no se aplica (no hay CCOR que evaluar). El desdoblado
//!   dual-PRF se marca `ray_flag::DEALIAS_FAILED` sólo cuando el
//!   emparejamiento entre radiales consecutivos no es posible (primer
//!   radial tras `START`/config, o el DRx no alternó `prf_div`);
//!   staggered-PRT nunca lo marca, porque el desdoblado es autocontenido
//!   dentro de un solo radial. En ninguno de los dos modos se evalúa aparte
//!   la convergencia por celda (residuo de
//!   `lamula_dual_prf::dealias_dual_prf`) — no hay umbral de "residuo
//!   aceptable" documentado en este repo. La conversión `Config` (`prf_hz`,
//!   `prf_ratio_num`/`den`) → `T1`,`T2` de staggered-PRT es una inferencia
//!   sin respaldo de oráculo (`crate::ray::staggered_prt_split`, ver su
//!   doc-comment).
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
    // Radial anterior para desdoblado dual-PRF (`crate::ray::PreviousPrf`):
    // se reinicia junto con `assembler` porque emparejar con un radial de
    // antes de un `START`/config nuevo no tiene sentido físico.
    let mut previous_prf: Option<ray::PreviousPrf> = None;
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
                        let (msg, next_previous_prf) = ray::build_moment_ray(
                            &radial,
                            cfg_snapshot,
                            ray_seq,
                            first_ray_after_config,
                            cfg.ssi_counts_per_turn,
                            cfg.ssi_zero_offset_deg,
                            previous_prf.as_ref(),
                        );
                        previous_prf = Some(next_previous_prf);
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
                    &mut previous_prf,
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

#[allow(clippy::too_many_arguments)]
async fn handle_down_message(
    msg: DownMessage,
    session: &mut Session,
    up: &mpsc::Sender<UpMessage>,
    assembler: &mut Option<RadialAssembler>,
    first_ray_after_config: &mut bool,
    previous_prf: &mut Option<ray::PreviousPrf>,
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
                    *previous_prf = None;
                }
                command::STOP | command::ENTER_SETUP => {
                    *assembler = None;
                    *previous_prf = None;
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

/// UZ+CZ+V (pulse-pair o espectral, `estimator_mask`) más SQI+SIG (censura,
/// siempre pulse-pair, `crate::ray`), ZDR+ρHV+ΦDP+KDP (canal 1, cuando el
/// radial lo trae) y desdoblado dual-PRF/staggered-PRT: ver el doc-comment de
/// `crate` para por qué no hay más. `max_gates`/`max_pulses` son el techo del
/// tipo de cable (`n_gates`/`n_pulses` son `u16`), no un límite de hardware
/// medido — ningún documento del repo da uno real.
fn capabilities() -> Capabilities {
    Capabilities {
        moment_mask: (1 << moment_kind::UZ)
            | (1 << moment_kind::CZ)
            | (1 << moment_kind::V)
            | (1 << moment_kind::SQI)
            | (1 << moment_kind::SIG)
            | (1 << moment_kind::ZDR)
            | (1 << moment_kind::RHOHV)
            | (1 << moment_kind::PHIDP)
            | (1 << moment_kind::KDP),
        dealias_mask: 1 << dealias_mode::NONE
            | 1 << dealias_mode::DUAL_PRF
            | 1 << dealias_mode::STAGGERED_PRT,
        estimator_mask: 1 << estimator::PULSE_PAIR | 1 << estimator::SPECTRAL,
        max_gates: u16::MAX as u32,
        max_pulses: u16::MAX,
        n_rx_channels: 2,
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
        // Igual que `capabilities`: techo que este binario sabe procesar,
        // no una cuenta real de canales conectados (sin fuente para eso).
        n_rx_channels: 2,
        // severity/last_error/capability_flags/bite_flags/queue_depth/
        // bins_ok/bins_total/trigger_period_meas_ns/dc_offset_*/
        // noise_floor_dbm_{1,2,3}: sin fuente real en este workspace, ver
        // el doc-comment de `crate`.
        ..Default::default()
    }
}
