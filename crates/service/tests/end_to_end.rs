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

use lamula_contract::drx_dsp;
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
    buf.push(cfg.range_dealias_mode);
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
    buf.extend_from_slice(&cfg.antenna_isolation_db.to_le_bytes());
    buf.extend_from_slice(&cfg.wavelength_m.to_le_bytes());
    buf.push(cfg.polarization_mode);
    buf.push(cfg.transmitter_type);
    buf.extend_from_slice(&cfg.burst_window_bins.to_le_bytes());
    buf.push(cfg.pulse_width_idx);
    buf.push(cfg.cell_mode);
    buf.extend_from_slice(&cfg.prf_div.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_delay_0.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_delay_1.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_delay_2.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_delay_3.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_width_0.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_width_1.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_width_2.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_width_3.to_le_bytes());
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

/// Lee un `drx_dsp::Config` (issue #1 ítem 5) del socket falso del DRx y lo
/// devuelve ya decodificado — mismo layout que
/// `crates/ingest/src/wire.rs::encode_config_frame`, del otro lado.
async fn read_drx_config(stream: &mut TcpStream) -> drx_dsp::Config {
    let mut header = [0u8; drx_dsp::HEADER_SIZE];
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!(&header[0..4], &drx_dsp::MAGIC.to_le_bytes());
    assert_eq!(header[4], drx_dsp::VERSION_MAJOR);
    assert_eq!(header[6], drx_dsp::MsgType::Config as u8);
    let payload_len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    assert_eq!(payload_len, drx_dsp::CONFIG_SIZE);
    let mut body = vec![0u8; payload_len];
    stream.read_exact(&mut body).await.unwrap();
    drx_dsp::Config {
        seq: u32::from_le_bytes(body[0..4].try_into().unwrap()),
        prf_div: u32::from_le_bytes(body[4..8].try_into().unwrap()),
        range_bins: u16::from_le_bytes(body[8..10].try_into().unwrap()),
        pulse_width_idx: body[10],
        pulse_mode: body[11],
        cell_mode: body[12],
        channel_mask: body[13],
        scan_mode: body[14],
        pad0: body[15],
        trigger_delay_0: u32::from_le_bytes(body[16..20].try_into().unwrap()),
        trigger_delay_1: u32::from_le_bytes(body[20..24].try_into().unwrap()),
        trigger_delay_2: u32::from_le_bytes(body[24..28].try_into().unwrap()),
        trigger_delay_3: u32::from_le_bytes(body[28..32].try_into().unwrap()),
        trigger_width_0: u32::from_le_bytes(body[32..36].try_into().unwrap()),
        trigger_width_1: u32::from_le_bytes(body[36..40].try_into().unwrap()),
        trigger_width_2: u32::from_le_bytes(body[40..44].try_into().unwrap()),
        trigger_width_3: u32::from_le_bytes(body[44..48].try_into().unwrap()),
    }
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
    // Este test hace de RCP, y el RCP es el servidor de su enlace ("el
    // productor conecta"): escucha antes de arrancar el binario, que conecta
    // hacia aquí. Se queda con el listener en vez de usar `free_port()` para
    // no abrir una ventana entre elegir el puerto y ocuparlo.
    let rcp_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("no se pudo escuchar como RCP");
    let rcp_port = rcp_listener.local_addr().unwrap().port();

    let child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_lamula-dsp"))
            .env("LAMULA_DSP_DRX_ADDR", format!("127.0.0.1:{drx_port}"))
            .env("LAMULA_DSP_RCP_ADDR", format!("127.0.0.1:{rcp_port}"))
            .env("LAMULA_DSP_FULL_SCALE_COUNTS", FULL_SCALE.to_string())
            .env("LAMULA_DSP_SSI_COUNTS_PER_TURN", "4096")
            .env("LAMULA_DSP_SSI_ZERO_OFFSET_DEG", "0.0")
            .env("LAMULA_DSP_DRX_NCO_FS_HZ", "250000000.0")
            .env("LAMULA_DSP_DRX_NCO_WORD_BITS", "32")
            .env("LAMULA_DSP_DRX_TRIGGER_FS_HZ", "250000000.0")
            .env("LAMULA_DSP_TX_IF_HZ", "60000000.0")
            .env("LAMULA_DSP_RX_IF_HZ", "60000000.0")
            .env("LAMULA_DSP_AFC_TAU_S", "2.0")
            .env("LAMULA_DSP_AFC_AMP_THRESHOLD", "0.01")
            // Este test alimenta el binario con un DRx de mentira, así que
            // la procedencia es simulada: la cabecera de cada trama `up`
            // tiene que salir marcada, y se comprueba más abajo.
            .env("LAMULA_DSP_SIMULATED_SOURCE", "true")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("no se pudo arrancar el binario del servicio"),
    );

    // Mismo presupuesto de arranque que `connect_with_retries` (3 s): sin
    // límite, un binario que no arranca colgaría el test en vez de fallarlo.
    let (mut rcp, _rcp_peer) = tokio::time::timeout(Duration::from_secs(3), rcp_listener.accept())
        .await
        .expect("el binario del servicio no conectó al RCP a tiempo")
        .expect("accept falló");
    let mut drx = connect_with_retries(drx_port).await;

    let config = Config {
        seq: 1,
        moment_mask: (1 << dsp_rcp::moment_kind::UZ) | (1 << dsp_rcp::moment_kind::V),
        n_pulses: 64,
        n_gates: 3,
        clutter_filter: dsp_rcp::clutter_filter::NONE,
        dealias_mode: dsp_rcp::dealias_mode::NONE,
        sweep_mode: dsp_rcp::sweep_mode::SPLIT_CUT,
        estimator: dsp_rcp::estimator::PULSE_PAIR,
        rfi_filter: 0,
        range_dealias_mode: 0,
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
        antenna_isolation_db: 0.0,
        wavelength_m: 0.10,
        polarization_mode: 0,
        transmitter_type: 0,
        burst_window_bins: 0,
        pulse_width_idx: 2,
        cell_mode: 1,
        prf_div: 40,
        trigger_delay_0: 4.0,
        trigger_delay_1: 0.0,
        trigger_delay_2: 0.0,
        trigger_delay_3: 0.0,
        trigger_width_0: 1.0,
        trigger_width_1: 0.0,
        trigger_width_2: 0.0,
        trigger_width_3: 0.0,
    };
    rcp.write_all(&build_config_frame(&config)).await.unwrap();
    assert_eq!(read_config_ack(&mut rcp).await, (1, dsp_rcp::error::OK));

    // Config válido y `sweep_mode` de tipo corte (`SPLIT_CUT`): el binario
    // debe retransmitir un `drx_dsp::Config` al DRx (issue #1 ítem 5,
    // `crate::ray::build_drx_config`). LAMULA_DSP_DRX_TRIGGER_FS_HZ=250 MHz
    // (arriba): 4.0 µs -> 1000 ciclos, 1.0 µs -> 250 ciclos.
    let drx_config = read_drx_config(&mut drx).await;
    // Copias locales: `drx_dsp::Config` es `packed`, tomar referencia a un
    // campo directamente (lo que hace `assert_eq!` por dentro) es UB.
    let (seq, range_bins, prf_div, pulse_width_idx, pulse_mode, cell_mode) = (
        drx_config.seq,
        drx_config.range_bins,
        drx_config.prf_div,
        drx_config.pulse_width_idx,
        drx_config.pulse_mode,
        drx_config.cell_mode,
    );
    let (channel_mask, scan_mode, trigger_delay_0, trigger_width_0) = (
        drx_config.channel_mask,
        drx_config.scan_mode,
        drx_config.trigger_delay_0,
        drx_config.trigger_width_0,
    );
    let (want_seq, want_n_gates, want_prf_div, want_pulse_width_idx, want_cell_mode) = (
        config.seq,
        config.n_gates,
        config.prf_div,
        config.pulse_width_idx,
        config.cell_mode,
    );
    assert_eq!(seq, want_seq);
    assert_eq!(range_bins, want_n_gates);
    assert_eq!(prf_div, want_prf_div);
    assert_eq!(pulse_width_idx, want_pulse_width_idx);
    assert_eq!(pulse_mode, 0);
    assert_eq!(cell_mode, want_cell_mode);
    assert_eq!(
        channel_mask,
        lamula_contract::drx_dsp::channel::RX_0,
        "sólo UZ+V pedidos, sin dual-pol ni burst: nada más que RX_0"
    );
    assert_eq!(scan_mode, 0, "SPLIT_CUT -> scan_mode 0");
    assert_eq!(trigger_delay_0, 1000);
    assert_eq!(trigger_width_0, 250);

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
    // El binario arrancó con LAMULA_DSP_SIMULATED_SOURCE=true: el radial sale
    // marcado como simulado. Es lo que impide que el RCP lo archive como
    // observación meteorológica.
    assert_eq!(
        header[7],
        dsp_rcp::header_flag::SIMULATED_SOURCE,
        "el moment_ray salió sin marca de procedencia simulada"
    );
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
