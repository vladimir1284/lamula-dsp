//! Comprueba la codificación de mensajes `up` contra el layout del contrato
//! (incluido el ejemplo del propio doc-comment de `payload_len`: 4 celdas,
//! 2 momentos → 152), y que `decode_down_frame` es el inverso exacto de una
//! trama `down` construida a mano campo a campo desde el esquema — la misma
//! disciplina que `contract/tests/test_dsp_rcp_codegen.py` aplica en Python.

use lamula_contract::dsp_rcp::{
    self, BiteEvent, Capabilities, Config, ConfigAck, Control, MomentField, MomentRay, MsgType,
    SelftestRequest, SelftestResult, Status, CONFIG_SIZE, CONTROL_SIZE, HEADER_SIZE, MAGIC,
    SELFTEST_REQUEST_SIZE, STATUS_SIZE, VERSION_MAJOR, VERSION_MINOR,
};
use lamula_rcp_link::wire::{
    decode_down_frame, encode_bite_event, encode_moment_ray, encode_status, DownMessage,
    MomentBlock,
};
use lamula_rcp_link::RcpLinkError;

fn zero_moment_ray(n_gates: u16, n_moments: u8) -> MomentRay {
    MomentRay {
        seq: 1,
        acq_time_utc_ns: 0,
        acq_monotonic_ns: 0,
        volume_seq: 0,
        sweep_seq: 0,
        ray_index: 0,
        n_gates,
        n_pulses: 0,
        bins_valid: n_gates,
        n_moments,
        sweep_mode: 0,
        prf_mode: 0,
        ray_flags: 0,
        pad0: 0,
        az_start_deg: 0.0,
        az_end_deg: 0.0,
        el_start_deg: 0.0,
        el_end_deg: 0.0,
        fixed_angle_deg: 0.0,
        start_range_m: 0.0,
        gate_spacing_m: 0.0,
        prf_hz: 0.0,
        nyquist_velocity: 0.0,
        unambiguous_range_m: 0.0,
        noise_floor_dbm: 0.0,
        radar_constant_db: 0.0,
    }
}

fn block(kind: u8, values: Vec<f32>) -> MomentBlock {
    let n_gates = values.len() as u32;
    MomentBlock {
        field: MomentField {
            kind,
            data_type: dsp_rcp::data_type::F32,
            flags: 0,
            pad0: 0,
            n_gates,
            scale: 1.0,
            offset: 0.0,
        },
        values,
    }
}

#[test]
fn moment_ray_matches_contract_docstring_example() {
    // `docs/contracts/index.md` / el esquema: "Para un moment_ray de 4
    // celdas y 2 momentos vale 88 + 2·(16 + 4·4) = 152, no 64."
    let ray = zero_moment_ray(4, 2);
    let moments = vec![
        block(dsp_rcp::moment_kind::UZ, vec![1.0, 2.0, 3.0, 4.0]),
        block(dsp_rcp::moment_kind::V, vec![5.0, 6.0, 7.0, 8.0]),
    ];

    let frame = encode_moment_ray(&ray, &moments);

    assert_eq!(frame.len(), HEADER_SIZE + 152);
    let magic = u32::from_le_bytes(frame[0..4].try_into().unwrap());
    assert_eq!(magic, MAGIC);
    assert_eq!(frame[6], MsgType::MomentRay as u8);
    let payload_len = u32::from_le_bytes(frame[8..12].try_into().unwrap());
    assert_eq!(payload_len, 152);

    let seq = u32::from_le_bytes(frame[12..16].try_into().unwrap());
    assert_eq!(seq, 1);

    // Primer valor del primer bloque: offset 12 (cabecera común) + 88
    // (MomentRay) + 16 (MomentField) = 116.
    let v0 = f32::from_le_bytes(frame[116..120].try_into().unwrap());
    assert_eq!(v0, 1.0);

    // Último valor del segundo bloque: 12 + 88 + 16 + 16 (4 valores del
    // primer bloque) + 16 (segundo MomentField) + 3*4 = 160..164.
    let v_last = f32::from_le_bytes(frame[160..164].try_into().unwrap());
    assert_eq!(v_last, 8.0);
}

#[test]
#[should_panic(expected = "n_moments")]
fn moment_ray_rejects_block_count_mismatch() {
    let ray = zero_moment_ray(4, 2);
    let moments = vec![block(dsp_rcp::moment_kind::UZ, vec![1.0, 2.0, 3.0, 4.0])];
    let _ = encode_moment_ray(&ray, &moments);
}

#[test]
fn status_is_fixed_size_with_no_variable_payload() {
    let status = Status {
        uptime_s: 42,
        phase: dsp_rcp::phase::RUNNING,
        severity: dsp_rcp::severity::WARNING,
        last_error: 0,
        n_rx_channels: 2,
        capability_flags: dsp_rcp::capability_flag::DUAL_POL,
        bite_flags: dsp_rcp::bite_flag::TRIGGER_DRIFT,
        config_seq: 3,
        rays_in: 100,
        rays_out: 99,
        rays_dropped: 1,
        queue_depth: 0,
        bins_ok: 900,
        bins_total: 1000,
        trigger_period_cmd_ns: 1_000_000,
        trigger_period_meas_ns: 1_000_050,
        pad0: 0,
        noise_floor_dbm_0: -110.0,
        noise_floor_dbm_1: -109.5,
        noise_floor_dbm_2: -110.2,
        noise_floor_dbm_3: -110.1,
        dc_offset_i_0: 0.01,
        dc_offset_i_1: 0.02,
        dc_offset_i_2: 0.0,
        dc_offset_i_3: -0.01,
        dc_offset_q_0: 0.0,
        dc_offset_q_1: 0.0,
        dc_offset_q_2: 0.0,
        dc_offset_q_3: 0.0,
    };

    let frame = encode_status(&status);

    assert_eq!(frame.len(), HEADER_SIZE + STATUS_SIZE);
    assert_eq!(frame[6], MsgType::Status as u8);
    let payload_len = u32::from_le_bytes(frame[8..12].try_into().unwrap());
    assert_eq!(payload_len, STATUS_SIZE as u32);
    let uptime = u32::from_le_bytes(frame[12..16].try_into().unwrap());
    assert_eq!(uptime, 42);
}

#[test]
fn bite_event_roundtrips_text_length() {
    let text = "trigger drift 50 ns";
    let event = BiteEvent {
        event_time_utc_ns: 123,
        code: dsp_rcp::bite_flag::TRIGGER_DRIFT,
        value: 50,
        severity: dsp_rcp::severity::WARNING,
        subsystem: 1,
        text_len: text.len() as u8,
        pad0: 0,
    };

    let frame = encode_bite_event(&event, text);

    let payload_len = u32::from_le_bytes(frame[8..12].try_into().unwrap()) as usize;
    assert_eq!(payload_len, dsp_rcp::BITE_EVENT_SIZE + text.len());
    let got_text = std::str::from_utf8(&frame[frame.len() - text.len()..]).unwrap();
    assert_eq!(got_text, text);
}

// --- mensajes `down`: construidos a mano campo a campo, como lo haría el
// RCP, para probar que `decode_down_frame` es su inverso exacto. ---

fn header_bytes(msg_type: MsgType, payload_len: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE);
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.push(VERSION_MAJOR);
    buf.push(VERSION_MINOR);
    buf.push(msg_type as u8);
    buf.push(0);
    buf.extend_from_slice(&payload_len.to_le_bytes());
    buf
}

fn build_config_frame(cfg: &Config) -> Vec<u8> {
    let mut buf = header_bytes(MsgType::Config, CONFIG_SIZE as u32);
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
    buf.extend_from_slice(&cfg.antenna_isolation_db.to_le_bytes());
    buf.extend_from_slice(&cfg.wavelength_m.to_le_bytes());
    buf.push(cfg.polarization_mode);
    buf.push(cfg.pad0);
    buf.extend_from_slice(&cfg.burst_window_bins.to_le_bytes());
    assert_eq!(buf.len(), HEADER_SIZE + CONFIG_SIZE);
    buf
}

#[test]
fn decode_config_is_inverse_of_hand_built_frame() {
    let cfg = Config {
        seq: 7,
        moment_mask: dsp_rcp::capability_flag::DUAL_POL,
        n_pulses: 64,
        n_gates: 1000,
        clutter_filter: dsp_rcp::clutter_filter::GMAP,
        dealias_mode: dsp_rcp::dealias_mode::DUAL_PRF,
        sweep_mode: dsp_rcp::sweep_mode::PPI,
        estimator: dsp_rcp::estimator::PULSE_PAIR,
        rfi_filter: 1,
        range_dealias: 0,
        prf_ratio_num: 4,
        prf_ratio_den: 5,
        start_range_m: 150.0,
        gate_spacing_m: 250.0,
        prf_hz: 1200.0,
        sqi_threshold: 0.4,
        sig_threshold: 3.0,
        ccor_threshold: 20.0,
        log_threshold: -10.0,
        clutter_width_ms: 1.5,
        radar_constant_db: 65.0,
        noise_floor_dbm: -108.0,
        receiver_gain_db: 40.0,
        zdr_offset_db: 0.2,
        phidp_offset_deg: 3.5,
        antenna_isolation_db: 0.0,
        wavelength_m: 0.1,
        polarization_mode: 0,
        pad0: 0,
        burst_window_bins: 0,
    };
    let frame = build_config_frame(&cfg);

    let decoded = decode_down_frame(&frame).unwrap();
    assert_eq!(decoded, DownMessage::Config(cfg));
}

#[test]
fn decode_control_is_inverse_of_hand_built_frame() {
    let control = Control {
        seq: 9,
        command: dsp_rcp::command::START,
        pad0: 0,
        pad1: 0,
    };
    let mut buf = header_bytes(MsgType::Control, CONTROL_SIZE as u32);
    buf.extend_from_slice(&control.seq.to_le_bytes());
    buf.push(control.command);
    buf.push(control.pad0);
    buf.extend_from_slice(&control.pad1.to_le_bytes());

    let decoded = decode_down_frame(&buf).unwrap();
    assert_eq!(decoded, DownMessage::Control(control));
}

#[test]
fn decode_selftest_request_is_inverse_of_hand_built_frame() {
    let req = SelftestRequest {
        seq: 11,
        nonce: 0xDEADBEEF,
    };
    let mut buf = header_bytes(MsgType::SelftestRequest, SELFTEST_REQUEST_SIZE as u32);
    buf.extend_from_slice(&req.seq.to_le_bytes());
    buf.extend_from_slice(&req.nonce.to_le_bytes());

    let decoded = decode_down_frame(&buf).unwrap();
    assert_eq!(decoded, DownMessage::SelftestRequest(req));
}

#[test]
fn decode_rejects_bad_magic() {
    let control = Control {
        seq: 1,
        command: dsp_rcp::command::STOP,
        pad0: 0,
        pad1: 0,
    };
    let mut buf = header_bytes(MsgType::Control, CONTROL_SIZE as u32);
    buf[0] = 0xFF; // corrompe el magic
    buf.extend_from_slice(&control.seq.to_le_bytes());
    buf.push(control.command);
    buf.push(control.pad0);
    buf.extend_from_slice(&control.pad1.to_le_bytes());

    assert!(matches!(
        decode_down_frame(&buf),
        Err(RcpLinkError::BadMagic { .. })
    ));
}

#[test]
fn decode_rejects_truncated_frame() {
    let buf = header_bytes(MsgType::Control, CONTROL_SIZE as u32);
    // Cabecera completa, pero sin el cuerpo declarado en payload_len.
    assert!(matches!(
        decode_down_frame(&buf),
        Err(RcpLinkError::Truncated)
    ));
}

/// `Capabilities`/`ConfigAck`/`SelftestResult` no tienen sentido `down`, así
/// que no hace falta decodificarlos aquí; se comprueba en cambio que
/// codifican al tamaño fijo declarado en el esquema, cerrando la cobertura
/// de los siete mensajes `up`.
#[test]
fn config_ack_and_selftest_result_and_capabilities_are_fixed_size() {
    use lamula_rcp_link::wire::{encode_capabilities, encode_config_ack, encode_selftest_result};

    let ack = ConfigAck {
        seq: 1,
        error: dsp_rcp::error::OK,
        pad0: 0,
        pad1: 0,
    };
    assert_eq!(
        encode_config_ack(&ack).len(),
        HEADER_SIZE + dsp_rcp::CONFIG_ACK_SIZE
    );

    let result = SelftestResult {
        seq: 1,
        nonce: 5,
        capability_flags: 0,
        error: dsp_rcp::error::OK,
        version_major: VERSION_MAJOR,
        version_minor: VERSION_MINOR,
        pad0: 0,
    };
    assert_eq!(
        encode_selftest_result(&result).len(),
        HEADER_SIZE + dsp_rcp::SELFTEST_RESULT_SIZE
    );

    let caps = Capabilities {
        moment_mask: dsp_rcp::capability_flag::DUAL_POL,
        dealias_mask: dsp_rcp::capability_flag::DUAL_PRF,
        estimator_mask: 0,
        max_gates: 2000,
        max_pulses: 128,
        n_rx_channels: 2,
        pad0: 0,
    };
    assert_eq!(
        encode_capabilities(&caps).len(),
        HEADER_SIZE + dsp_rcp::CAPABILITIES_SIZE
    );
}
