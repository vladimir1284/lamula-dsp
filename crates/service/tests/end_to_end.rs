//! Prueba de humo del binario real: lo arranca como subproceso y le conecta
//! un DRx y un RCP falsos por TCP, comprobando que el flujo de control
//! (`config` → `config_ack`, `start` → `config_ack`) y el de datos (radial
//! del DRx → `moment_ray` al RCP) atraviesan el proceso completo — no sólo
//! el ensamblado en memoria que ya cubre
//! `crates/rcp-link/tests/vertical_slice.rs`.
//!
//! Mismas simplificaciones que ese test: un solo canal, sin barrido
//! simulado, sólo UZ+V.

use std::net::TcpListener as StdTcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use lamula_contract::dsp_rcp::{self, Config, Control, MsgType, HEADER_SIZE, MAGIC};
use lamula_simulator::{generate_cell, pack_rays, CellParams, RayHeaderFields};
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::sleep;

const FULL_SCALE: i16 = i16::MAX;

/// Mata al subproceso al salir del test, pase lo que pase (asertos
/// incluidos): si no, un `assert!` fallido deja al binario huérfano.
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    StdTcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn header_bytes(msg_type: MsgType, payload_len: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE);
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.push(dsp_rcp::VERSION_MAJOR);
    buf.push(dsp_rcp::VERSION_MINOR);
    buf.push(msg_type as u8);
    buf.push(0);
    buf.extend_from_slice(&payload_len.to_le_bytes());
    buf
}

fn build_config_frame(cfg: &Config) -> Vec<u8> {
    let mut buf = header_bytes(MsgType::Config, dsp_rcp::CONFIG_SIZE as u32);
    buf.extend_from_slice(&cfg.seq.to_le_bytes());
    buf.extend_from_slice(&cfg.moment_mask.to_le_bytes());
    buf.extend_from_slice(&cfg.n_pulses.to_le_bytes());
    buf.extend_from_slice(&cfg.n_gates.to_le_bytes());
    buf.push(cfg.clutter_filter);
    buf.push(cfg.dealias_mode);
    buf.push(cfg.sweep_mode);
    buf.push(cfg.estimator);
    buf.push(cfg.rfi_filter);
    buf.push(cfg.range_dealias);
    buf.push(cfg.prf_ratio_num);
    buf.push(cfg.prf_ratio_den);
    buf.extend_from_slice(&cfg.start_range_m.to_le_bytes());
    buf.extend_from_slice(&cfg.gate_spacing_m.to_le_bytes());
    buf.extend_from_slice(&cfg.prf_hz.to_le_bytes());
    buf.extend_from_slice(&cfg.sqi_threshold.to_le_bytes());
    buf.extend_from_slice(&cfg.sig_threshold.to_le_bytes());
    buf.extend_from_slice(&cfg.ccor_threshold.to_le_bytes());
    buf.extend_from_slice(&cfg.log_threshold.to_le_bytes());
    buf.extend_from_slice(&cfg.clutter_width_ms.to_le_bytes());
    buf.extend_from_slice(&cfg.radar_constant_db.to_le_bytes());
    buf.extend_from_slice(&cfg.noise_floor_dbm.to_le_bytes());
    buf.extend_from_slice(&cfg.receiver_gain_db.to_le_bytes());
    buf.extend_from_slice(&cfg.zdr_offset_db.to_le_bytes());
    buf.extend_from_slice(&cfg.phidp_offset_deg.to_le_bytes());
    buf.extend_from_slice(&cfg.wavelength_m.to_le_bytes());
    buf.push(cfg.polarization_mode);
    buf.push(cfg.pad0);
    buf.extend_from_slice(&cfg.pad1.to_le_bytes());
    buf
}

fn build_control_frame(control: &Control) -> Vec<u8> {
    let mut buf = header_bytes(MsgType::Control, dsp_rcp::CONTROL_SIZE as u32);
    buf.extend_from_slice(&control.seq.to_le_bytes());
    buf.push(control.command);
    buf.push(control.pad0);
    buf.extend_from_slice(&control.pad1.to_le_bytes());
    buf
}

async fn read_config_ack(stream: &mut TcpStream) -> (u32, u8) {
    let mut header = [0u8; HEADER_SIZE];
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!(header[6], MsgType::ConfigAck as u8);
    let payload_len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).await.unwrap();
    let seq = u32::from_le_bytes(payload[0..4].try_into().unwrap());
    (seq, payload[4])
}

async fn connect_with_retries(port: u16) -> TcpStream {
    for _ in 0..150 {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", port)).await {
            return s;
        }
        sleep(Duration::from_millis(20)).await;
    }
    panic!("no se pudo conectar a 127.0.0.1:{port}: el binario no arrancó a tiempo");
}

#[tokio::test]
async fn service_binary_wires_drx_to_rcp() {
    let drx_port = free_port();
    let rcp_port = free_port();

    let child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_lamula-dsp"))
            .env("LAMULA_DSP_DRX_ADDR", format!("127.0.0.1:{drx_port}"))
            .env("LAMULA_DSP_RCP_ADDR", format!("127.0.0.1:{rcp_port}"))
            .env("LAMULA_DSP_FULL_SCALE_COUNTS", FULL_SCALE.to_string())
            .env("LAMULA_DSP_SSI_COUNTS_PER_TURN", "4096")
            .env("LAMULA_DSP_SSI_ZERO_OFFSET_DEG", "0.0")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("no se pudo arrancar el binario del servicio"),
    );

    let mut rcp = connect_with_retries(rcp_port).await;
    let mut drx = connect_with_retries(drx_port).await;

    let config = Config {
        seq: 1,
        moment_mask: (1 << dsp_rcp::moment_kind::UZ) | (1 << dsp_rcp::moment_kind::V),
        n_pulses: 64,
        n_gates: 3,
        clutter_filter: dsp_rcp::clutter_filter::NONE,
        dealias_mode: dsp_rcp::dealias_mode::NONE,
        sweep_mode: dsp_rcp::sweep_mode::PPI,
        estimator: dsp_rcp::estimator::PULSE_PAIR,
        rfi_filter: 0,
        range_dealias: 0,
        prf_ratio_num: 0,
        prf_ratio_den: 0,
        start_range_m: 0.0,
        gate_spacing_m: 250.0,
        prf_hz: 1000.0,
        sqi_threshold: 0.4,
        sig_threshold: 3.0,
        ccor_threshold: 20.0,
        log_threshold: -10.0,
        clutter_width_ms: 1.0,
        radar_constant_db: 65.0,
        noise_floor_dbm: -108.0,
        receiver_gain_db: 40.0,
        zdr_offset_db: 0.0,
        phidp_offset_deg: 0.0,
        wavelength_m: 0.10,
        polarization_mode: 0,
        pad0: 0,
        pad1: 0,
    };
    rcp.write_all(&build_config_frame(&config)).await.unwrap();
    assert_eq!(read_config_ack(&mut rcp).await, (1, dsp_rcp::error::OK));

    let start = Control {
        seq: 2,
        command: dsp_rcp::command::START,
        pad0: 0,
        pad1: 0,
    };
    rcp.write_all(&build_control_frame(&start)).await.unwrap();
    assert_eq!(read_config_ack(&mut rcp).await, (2, dsp_rcp::error::OK));

    // Ya en `running`: manda un radial de 3 celdas, 64 pulsos, por el DRx
    // falso — n_pulses tiene que casar con `config.n_pulses` para que el
    // `RadialAssembler` del servicio complete un radial.
    let prt_s = 1.0 / config.prf_hz as f64;
    let mut rng = StdRng::seed_from_u64(7);
    let params = CellParams {
        power_s: 0.01,
        mean_v: 5.0,
        sigma_v: 1.0,
        wavelength_m: config.wavelength_m as f64,
        prt_s,
        m: config.n_pulses as usize,
        noise_floor: 0.0,
    };
    let cells: Vec<_> = (0..config.n_gates)
        .map(|_| generate_cell(&params, &mut rng))
        .collect();
    let fields = RayHeaderFields {
        seq_start: 0,
        timestamp_ns_start: 0,
        timestamp_step_ns: (prt_s * 1.0e9) as u64,
        trigger_count_start: 0,
        azimuth_raw: 512,
        elevation_raw: 0,
        prf_div: 4,
        pulse_width_idx: 0,
        pulse_mode: 0,
        cell_mode: 0,
        channel_mask: 0b0001,
        ray_flags: 0,
    };
    let wire_frames = pack_rays(&fields, &[cells], FULL_SCALE);
    for frame in &wire_frames {
        drx.write_all(frame).await.unwrap();
    }

    // Espera el `moment_ray` resultante al otro lado del enlace RCP.
    let mut header = [0u8; HEADER_SIZE];
    rcp.read_exact(&mut header).await.unwrap();
    assert_eq!(header[6], MsgType::MomentRay as u8);
    let payload_len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    let mut payload = vec![0u8; payload_len];
    rcp.read_exact(&mut payload).await.unwrap();

    // Cabecera fija del moment_ray: 88 B (ver MOMENT_RAY_SIZE).
    let got_n_gates = u16::from_le_bytes(payload[28..30].try_into().unwrap());
    let want_n_gates = config.n_gates;
    assert_eq!(got_n_gates, want_n_gates);
    let got_n_pulses = u16::from_le_bytes(payload[30..32].try_into().unwrap());
    let want_n_pulses = config.n_pulses;
    assert_eq!(got_n_pulses, want_n_pulses);
    let got_n_moments = payload[34];
    assert_eq!(got_n_moments, 2); // UZ + V, los dos pedidos en moment_mask

    drop(rcp);
    drop(drx);
    drop(child);
}
