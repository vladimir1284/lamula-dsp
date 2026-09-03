//! Disposición real de los dos contratos, comprobada contra el compilador.
//!
//! El proyecto LAMULA DRx verifica su codegen comparando byte a byte la salida
//! de C con la de Python, pero deja fuera el lado Rust porque en aquel
//! repositorio no hay toolchain de Rust; su documentación dice explícitamente
//! que el test de disposición de Rust es responsabilidad del proyecto DSP. Este
//! fichero es ese test, y de paso cubre igual el contrato propio DSP↔RCP.
//!
//! Es la mitad que el test de Python (`contract/tests/test_drx_dsp_layout.py`)
//! no puede hacer: allí se comprueba que el fichero *generado* dice lo que debe,
//! aquí se comprueba lo que el compilador *hace* con él. Son fallos distintos —
//! un `#[repr(C, packed)]` que se perdiera en una regeneración pasaría el test
//! de Python y rompería este.
//!
//! Los anchos se reescriben a mano, campo por campo. La comprobación no es
//! «el desplazamiento es el que dice el struct» —eso es una tautología— sino
//! «el desplazamiento es la suma de los anchos de los campos anteriores», que
//! es lo que significa empaquetado sin relleno implícito.

use lamula_contract::{drx_dsp, dsp_rcp};

/// Comprueba una estructura entera: desplazamiento de cada campo como suma
/// acumulada de anchos, que los anchos sumen el tamaño declarado, y que
/// `size_of` coincida. Si el compilador insertara un solo byte de relleno,
/// falla en el primer campo posterior.
macro_rules! check_layout {
    ($t:ty, $total:expr, $( $field:ident : $w:expr ),+ $(,)?) => {{
        let mut off = 0usize;
        $(
            assert_eq!(
                core::mem::offset_of!($t, $field),
                off,
                concat!(stringify!($t), ".", stringify!($field),
                        ": desplazamiento inesperado")
            );
            off += $w;
        )+
        assert_eq!(
            off, $total,
            concat!(stringify!($t), ": los anchos no suman el tamaño declarado")
        );
        assert_eq!(
            core::mem::size_of::<$t>(), $total,
            concat!(stringify!($t), ": size_of no coincide; ¿se perdió repr(packed)?")
        );
    }};
}

// ---------------------------------------------------------------------------
// DRx↔DSP v0.3 — vendorizado, congelado por D-08 del proyecto DRx.
// ---------------------------------------------------------------------------

#[test]
fn drx_identidad() {
    assert_eq!(drx_dsp::MAGIC, 0x4C4D_4452, "magic no es \"LMDR\"");
    assert_eq!(drx_dsp::VERSION_MAJOR, 0);
    assert_eq!(drx_dsp::VERSION_MINOR, 3);
}

#[test]
fn drx_tipos_de_mensaje() {
    assert_eq!(drx_dsp::MsgType::Ray as u8, 1);
    assert_eq!(drx_dsp::MsgType::Status as u8, 2);
    assert_eq!(drx_dsp::MsgType::Config as u8, 3);
    assert_eq!(drx_dsp::MsgType::ConfigAck as u8, 4);
    assert_eq!(drx_dsp::MsgType::Afc as u8, 5);
}

#[test]
fn drx_header() {
    check_layout!(
        drx_dsp::Header, 12,
        magic: 4, version_major: 1, version_minor: 1, msg_type: 1, flags: 1,
        payload_len: 4,
    );
    assert_eq!(drx_dsp::HEADER_SIZE, 12);
}

#[test]
fn drx_ray() {
    check_layout!(
        drx_dsp::Ray, 36,
        seq: 4, timestamp_ns: 8, trigger_count: 4, azimuth_raw: 4,
        elevation_raw: 4, prf_div: 4, bins: 2, pulse_width_idx: 1,
        pulse_mode: 1, cell_mode: 1, n_channels: 1, channel_mask: 1,
        ray_flags: 1,
    );
    assert_eq!(drx_dsp::RAY_SIZE, 36);
}

#[test]
fn drx_status() {
    check_layout!(
        drx_dsp::Status, 28,
        uptime_s: 4, bite_flags: 4, ssa_underruns: 4, dma_overruns: 4,
        ssi_errors: 4, ddc_overflows: 4, last_error: 1, pad0: 1, pad1: 2,
    );
    assert_eq!(drx_dsp::STATUS_SIZE, 28);
}

#[test]
fn drx_config() {
    check_layout!(
        drx_dsp::Config, 48,
        seq: 4, prf_div: 4, range_bins: 2, pulse_width_idx: 1, pulse_mode: 1,
        cell_mode: 1, channel_mask: 1, scan_mode: 1, pad0: 1,
        trigger_delay_0: 4, trigger_delay_1: 4, trigger_delay_2: 4,
        trigger_delay_3: 4, trigger_width_0: 4, trigger_width_1: 4,
        trigger_width_2: 4, trigger_width_3: 4,
    );
    assert_eq!(drx_dsp::CONFIG_SIZE, 48);
}

#[test]
fn drx_config_ack() {
    check_layout!(drx_dsp::ConfigAck, 8, seq: 4, error: 1, pad0: 1, pad1: 2);
    assert_eq!(drx_dsp::CONFIG_ACK_SIZE, 8);
}

#[test]
fn drx_afc() {
    check_layout!(drx_dsp::Afc, 16, nco_phase_inc: 8, apply_at_seq: 4, pad0: 4);
    assert_eq!(drx_dsp::AFC_SIZE, 16);
}

// ---------------------------------------------------------------------------
// DSP↔RCP v0.1 — propio.
// ---------------------------------------------------------------------------

#[test]
fn dsp_identidad() {
    assert_eq!(dsp_rcp::MAGIC, 0x4C4D_4453, "magic no es \"LMDS\"");
    assert_eq!(dsp_rcp::VERSION_MAJOR, 1);
    assert_eq!(dsp_rcp::VERSION_MINOR, 1);
}

/// Los dos contratos comparten forma y tamaño de cabecera a propósito, para que
/// un solo lector de tramas sirva en los dos enlaces. Lo que NO comparten es el
/// magic: si coincidieran, un cable mal conectado pasaría desapercibido.
#[test]
fn cabeceras_compatibles_pero_magic_distinto() {
    assert_eq!(drx_dsp::HEADER_SIZE, dsp_rcp::HEADER_SIZE);
    assert_ne!(drx_dsp::MAGIC, dsp_rcp::MAGIC);
    assert_eq!(
        core::mem::offset_of!(drx_dsp::Header, payload_len),
        core::mem::offset_of!(dsp_rcp::Header, payload_len),
    );
}

#[test]
fn dsp_tipos_de_mensaje() {
    assert_eq!(dsp_rcp::MsgType::MomentRay as u8, 1);
    assert_eq!(dsp_rcp::MsgType::SpectrumFrame as u8, 2);
    assert_eq!(dsp_rcp::MsgType::Status as u8, 3);
    assert_eq!(dsp_rcp::MsgType::BiteEvent as u8, 4);
    assert_eq!(dsp_rcp::MsgType::ConfigAck as u8, 5);
    assert_eq!(dsp_rcp::MsgType::SelftestResult as u8, 6);
    assert_eq!(dsp_rcp::MsgType::Capabilities as u8, 7);
    assert_eq!(dsp_rcp::MsgType::Config as u8, 8);
    assert_eq!(dsp_rcp::MsgType::Control as u8, 9);
    assert_eq!(dsp_rcp::MsgType::SelftestRequest as u8, 10);
}

#[test]
fn dsp_header() {
    check_layout!(
        dsp_rcp::Header, 12,
        magic: 4, version_major: 1, version_minor: 1, msg_type: 1, flags: 1,
        payload_len: 4,
    );
    assert_eq!(dsp_rcp::HEADER_SIZE, 12);
}

#[test]
fn dsp_moment_ray() {
    check_layout!(
        dsp_rcp::MomentRay, 88,
        seq: 4, acq_time_utc_ns: 8, acq_monotonic_ns: 8,
        volume_seq: 4, sweep_seq: 2, ray_index: 2,
        n_gates: 2, n_pulses: 2, bins_valid: 2, n_moments: 1, sweep_mode: 1,
        prf_mode: 1, ray_flags: 1, pad0: 2,
        az_start_deg: 4, az_end_deg: 4, el_start_deg: 4, el_end_deg: 4,
        fixed_angle_deg: 4, start_range_m: 4, gate_spacing_m: 4, prf_hz: 4,
        nyquist_velocity: 4, unambiguous_range_m: 4, noise_floor_dbm: 4,
        radar_constant_db: 4,
    );
    assert_eq!(dsp_rcp::MOMENT_RAY_SIZE, 88);
}

#[test]
fn dsp_moment_field() {
    check_layout!(
        dsp_rcp::MomentField, 16,
        kind: 1, data_type: 1, flags: 1, pad0: 1, n_gates: 4, scale: 4,
        offset: 4,
    );
    assert_eq!(dsp_rcp::MOMENT_FIELD_SIZE, 16);
}

#[test]
fn dsp_spectrum_frame() {
    check_layout!(
        dsp_rcp::SpectrumFrame, 32,
        seq: 4, capture_time_utc_ns: 8, n_bins: 2, channel: 1, flags: 1,
        center_freq_hz: 4, span_hz: 4, ref_level_dbm: 4, pad0: 4,
    );
    assert_eq!(dsp_rcp::SPECTRUM_FRAME_SIZE, 32);
}

#[test]
fn dsp_status() {
    check_layout!(
        dsp_rcp::Status, 104,
        uptime_s: 4, phase: 1, severity: 1, last_error: 1, n_rx_channels: 1,
        capability_flags: 4, bite_flags: 4, config_seq: 4, rays_in: 4,
        rays_out: 4, rays_dropped: 4, queue_depth: 4, bins_ok: 4,
        bins_total: 4, trigger_period_cmd_ns: 4, trigger_period_meas_ns: 4,
        pad0: 4,
        noise_floor_dbm_0: 4, noise_floor_dbm_1: 4, noise_floor_dbm_2: 4,
        noise_floor_dbm_3: 4,
        dc_offset_i_0: 4, dc_offset_i_1: 4, dc_offset_i_2: 4, dc_offset_i_3: 4,
        dc_offset_q_0: 4, dc_offset_q_1: 4, dc_offset_q_2: 4, dc_offset_q_3: 4,
    );
    assert_eq!(dsp_rcp::STATUS_SIZE, 104);
}

#[test]
fn dsp_bite_event() {
    check_layout!(
        dsp_rcp::BiteEvent, 20,
        event_time_utc_ns: 8, code: 4, value: 4, severity: 1, subsystem: 1,
        text_len: 1, pad0: 1,
    );
    assert_eq!(dsp_rcp::BITE_EVENT_SIZE, 20);
}

#[test]
fn dsp_config_ack() {
    check_layout!(dsp_rcp::ConfigAck, 8, seq: 4, error: 1, pad0: 1, pad1: 2);
    assert_eq!(dsp_rcp::CONFIG_ACK_SIZE, 8);
}

#[test]
fn dsp_selftest() {
    check_layout!(
        dsp_rcp::SelftestResult, 16,
        seq: 4, nonce: 4, capability_flags: 4, error: 1, version_major: 1,
        version_minor: 1, pad0: 1,
    );
    assert_eq!(dsp_rcp::SELFTEST_RESULT_SIZE, 16);

    check_layout!(dsp_rcp::SelftestRequest, 8, seq: 4, nonce: 4);
    assert_eq!(dsp_rcp::SELFTEST_REQUEST_SIZE, 8);
}

#[test]
fn dsp_capabilities() {
    check_layout!(
        dsp_rcp::Capabilities, 20,
        moment_mask: 4, dealias_mask: 4, estimator_mask: 4, max_gates: 4,
        max_pulses: 2, n_rx_channels: 1, pad0: 1,
    );
    assert_eq!(dsp_rcp::CAPABILITIES_SIZE, 20);
}

#[test]
fn dsp_config() {
    check_layout!(
        dsp_rcp::Config, 84,
        seq: 4, moment_mask: 4, n_pulses: 2, n_gates: 2, clutter_filter: 1,
        dealias_mode: 1, sweep_mode: 1, estimator: 1, rfi_filter: 1,
        range_dealias: 1, prf_ratio_num: 1, prf_ratio_den: 1,
        start_range_m: 4, gate_spacing_m: 4, prf_hz: 4, sqi_threshold: 4,
        sig_threshold: 4, ccor_threshold: 4, log_threshold: 4,
        clutter_width_ms: 4, radar_constant_db: 4, noise_floor_dbm: 4,
        receiver_gain_db: 4, zdr_offset_db: 4, phidp_offset_deg: 4,
        antenna_isolation_db: 4, wavelength_m: 4, polarization_mode: 1,
        pad0: 1, burst_window_bins: 2,
    );
    assert_eq!(dsp_rcp::CONFIG_SIZE, 84);
}

#[test]
fn dsp_control() {
    check_layout!(dsp_rcp::Control, 8, seq: 4, command: 1, pad0: 1, pad1: 2);
    assert_eq!(dsp_rcp::CONTROL_SIZE, 8);
}

/// La máscara de momentos es un `u32` con un bit por momento, así que el
/// vocabulario canónico no puede pasar de 32 entradas sin cambiar el tipo.
/// Con 14 hay margen, pero el límite conviene que falle solo el día que se cruce.
#[test]
fn el_vocabulario_de_momentos_cabe_en_la_mascara() {
    assert_eq!(
        dsp_rcp::moment_kind::Q,
        13,
        "última entrada del vocabulario"
    );
    assert!(
        (dsp_rcp::moment_kind::Q as u32) < 32,
        "moment_mask es u32: no caben más de 32 momentos"
    );
}

/// El plan (§6.1) separa fase de configuración y fase de marcha. Que sean
/// valores distintos es lo que permite rechazar un `config` en marcha.
#[test]
fn las_fases_son_distintas() {
    assert_ne!(dsp_rcp::phase::SETUP, dsp_rcp::phase::RUNNING);
    assert_ne!(dsp_rcp::phase::RUNNING, dsp_rcp::phase::FAULT);
    assert_eq!(dsp_rcp::error::NOT_IN_SETUP_PHASE, 4);
}
