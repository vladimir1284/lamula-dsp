//! Codificación de los mensajes `up` (DSP→RCP) y decodificación de los
//! `down` (RCP→DSP) del contrato `DSP↔RCP` v0.1
//! (`contract/schema/dsp_rcp_v0_1.toml`).
//!
//! `payload_len` de este contrato incluye la cabecera propia del mensaje más
//! su carga variable (a diferencia de `DRx↔DSP`, donde sólo cuenta la carga):
//! ver el doc-comment del campo en el esquema. Eso hace que leer una trama
//! completa sea el mismo procedimiento para los diez tipos de mensaje: 12 B
//! de cabecera común, luego exactamente `payload_len` bytes más.

use lamula_contract::dsp_rcp::{
    BiteEvent, Capabilities, Config, ConfigAck, Control, MomentField, MomentRay, MsgType,
    SelftestRequest, SelftestResult, SpectrumFrame, Status, BITE_EVENT_SIZE, CAPABILITIES_SIZE,
    CONFIG_ACK_SIZE, CONFIG_SIZE, CONTROL_SIZE, HEADER_SIZE, MAGIC, MOMENT_FIELD_SIZE,
    MOMENT_RAY_SIZE, SELFTEST_REQUEST_SIZE, SELFTEST_RESULT_SIZE, SPECTRUM_FRAME_SIZE, STATUS_SIZE,
    VERSION_MAJOR, VERSION_MINOR,
};

use crate::error::RcpLinkError;

/// Un bloque de momento a incrustar en un `moment_ray`: el descriptor y sus
/// `n_gates` valores. `field.n_gates` y `values.len()` tienen que coincidir
/// entre sí y con el `n_gates` del radial — [`encode_moment_ray`] lo
/// comprueba con `assert_eq!`, porque es un invariante de quien ensambla el
/// radial, no un dato que pueda llegar mal formado desde fuera del proceso.
pub struct MomentBlock {
    pub field: MomentField,
    pub values: Vec<f32>,
}

/// Un mensaje `up` (DSP→RCP), listo para [`encode_up_message`].
pub enum UpMessage {
    MomentRay {
        ray: MomentRay,
        moments: Vec<MomentBlock>,
    },
    SpectrumFrame {
        frame: SpectrumFrame,
        bins_db: Vec<f32>,
    },
    Status(Status),
    BiteEvent {
        event: BiteEvent,
        text: String,
    },
    ConfigAck(ConfigAck),
    SelftestResult(SelftestResult),
    Capabilities(Capabilities),
}

/// Un mensaje `down` (RCP→DSP) ya decodificado por [`decode_down_frame`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DownMessage {
    Config(Config),
    Control(Control),
    SelftestRequest(SelftestRequest),
}

fn write_header(buf: &mut Vec<u8>, msg_type: MsgType, payload_len: u32) {
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.push(VERSION_MAJOR);
    buf.push(VERSION_MINOR);
    buf.push(msg_type as u8);
    buf.push(0); // flags, reservado en v0.1
    buf.extend_from_slice(&payload_len.to_le_bytes());
}

/// Despacha un [`UpMessage`] al codificador correspondiente.
pub fn encode_up_message(msg: &UpMessage) -> Vec<u8> {
    match msg {
        UpMessage::MomentRay { ray, moments } => encode_moment_ray(ray, moments),
        UpMessage::SpectrumFrame { frame, bins_db } => encode_spectrum_frame(frame, bins_db),
        UpMessage::Status(status) => encode_status(status),
        UpMessage::BiteEvent { event, text } => encode_bite_event(event, text),
        UpMessage::ConfigAck(ack) => encode_config_ack(ack),
        UpMessage::SelftestResult(result) => encode_selftest_result(result),
        UpMessage::Capabilities(caps) => encode_capabilities(caps),
    }
}

/// Codifica un radial de momentos. `moments.len()` tiene que coincidir con
/// `ray.n_moments`, y cada bloque con `ray.n_gates`: son invariantes del
/// ensamblado, se comprueban con `assert_eq!` en vez de devolver `Result`,
/// igual que `lamula_simulator::pack_rays` con las suyas.
pub fn encode_moment_ray(ray: &MomentRay, moments: &[MomentBlock]) -> Vec<u8> {
    assert_eq!(
        moments.len(),
        ray.n_moments as usize,
        "n_moments del radial no coincide con los bloques dados"
    );
    for m in moments {
        assert_eq!(
            m.values.len(),
            ray.n_gates as usize,
            "el bloque de momento no tiene n_gates valores"
        );
        assert_eq!(
            m.field.n_gates as usize, ray.n_gates as usize,
            "el descriptor de momento no coincide con n_gates del radial"
        );
    }

    let payload_len = MOMENT_RAY_SIZE
        + moments
            .iter()
            .map(|m| MOMENT_FIELD_SIZE + m.values.len() * std::mem::size_of::<f32>())
            .sum::<usize>();
    let mut buf = Vec::with_capacity(HEADER_SIZE + payload_len);
    write_header(&mut buf, MsgType::MomentRay, payload_len as u32);

    buf.extend_from_slice(&ray.seq.to_le_bytes());
    buf.extend_from_slice(&ray.acq_time_utc_ns.to_le_bytes());
    buf.extend_from_slice(&ray.acq_monotonic_ns.to_le_bytes());
    buf.extend_from_slice(&ray.volume_seq.to_le_bytes());
    buf.extend_from_slice(&ray.sweep_seq.to_le_bytes());
    buf.extend_from_slice(&ray.ray_index.to_le_bytes());
    buf.extend_from_slice(&ray.n_gates.to_le_bytes());
    buf.extend_from_slice(&ray.n_pulses.to_le_bytes());
    buf.extend_from_slice(&ray.bins_valid.to_le_bytes());
    buf.push(ray.n_moments);
    buf.push(ray.sweep_mode);
    buf.push(ray.prf_mode);
    buf.push(ray.ray_flags);
    buf.extend_from_slice(&ray.pad0.to_le_bytes());
    buf.extend_from_slice(&ray.az_start_deg.to_le_bytes());
    buf.extend_from_slice(&ray.az_end_deg.to_le_bytes());
    buf.extend_from_slice(&ray.el_start_deg.to_le_bytes());
    buf.extend_from_slice(&ray.el_end_deg.to_le_bytes());
    buf.extend_from_slice(&ray.fixed_angle_deg.to_le_bytes());
    buf.extend_from_slice(&ray.start_range_m.to_le_bytes());
    buf.extend_from_slice(&ray.gate_spacing_m.to_le_bytes());
    buf.extend_from_slice(&ray.prf_hz.to_le_bytes());
    buf.extend_from_slice(&ray.nyquist_velocity.to_le_bytes());
    buf.extend_from_slice(&ray.unambiguous_range_m.to_le_bytes());
    buf.extend_from_slice(&ray.noise_floor_dbm.to_le_bytes());
    buf.extend_from_slice(&ray.radar_constant_db.to_le_bytes());
    debug_assert_eq!(buf.len(), HEADER_SIZE + MOMENT_RAY_SIZE);

    for m in moments {
        buf.push(m.field.kind);
        buf.push(m.field.data_type);
        buf.push(m.field.flags);
        buf.push(m.field.pad0);
        buf.extend_from_slice(&m.field.n_gates.to_le_bytes());
        buf.extend_from_slice(&m.field.scale.to_le_bytes());
        buf.extend_from_slice(&m.field.offset.to_le_bytes());
        for v in &m.values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    debug_assert_eq!(buf.len(), HEADER_SIZE + payload_len);
    buf
}

/// Codifica una traza del analizador de espectro de FI. `bins_db.len()`
/// tiene que coincidir con `frame.n_bins`.
pub fn encode_spectrum_frame(frame: &SpectrumFrame, bins_db: &[f32]) -> Vec<u8> {
    assert_eq!(
        bins_db.len(),
        frame.n_bins as usize,
        "bins_db no tiene n_bins valores"
    );

    let payload_len = SPECTRUM_FRAME_SIZE + std::mem::size_of_val(bins_db);
    let mut buf = Vec::with_capacity(HEADER_SIZE + payload_len);
    write_header(&mut buf, MsgType::SpectrumFrame, payload_len as u32);

    buf.extend_from_slice(&frame.seq.to_le_bytes());
    buf.extend_from_slice(&frame.capture_time_utc_ns.to_le_bytes());
    buf.extend_from_slice(&frame.n_bins.to_le_bytes());
    buf.push(frame.channel);
    buf.push(frame.flags);
    buf.extend_from_slice(&frame.center_freq_hz.to_le_bytes());
    buf.extend_from_slice(&frame.span_hz.to_le_bytes());
    buf.extend_from_slice(&frame.ref_level_dbm.to_le_bytes());
    buf.extend_from_slice(&frame.pad0.to_le_bytes());
    debug_assert_eq!(buf.len(), HEADER_SIZE + SPECTRUM_FRAME_SIZE);

    for v in bins_db {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    debug_assert_eq!(buf.len(), HEADER_SIZE + payload_len);
    buf
}

/// Codifica un `status`: tamaño fijo, sin carga variable.
pub fn encode_status(status: &Status) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE + STATUS_SIZE);
    write_header(&mut buf, MsgType::Status, STATUS_SIZE as u32);

    buf.extend_from_slice(&status.uptime_s.to_le_bytes());
    buf.push(status.phase);
    buf.push(status.severity);
    buf.push(status.last_error);
    buf.push(status.n_rx_channels);
    buf.extend_from_slice(&status.capability_flags.to_le_bytes());
    buf.extend_from_slice(&status.bite_flags.to_le_bytes());
    buf.extend_from_slice(&status.config_seq.to_le_bytes());
    buf.extend_from_slice(&status.rays_in.to_le_bytes());
    buf.extend_from_slice(&status.rays_out.to_le_bytes());
    buf.extend_from_slice(&status.rays_dropped.to_le_bytes());
    buf.extend_from_slice(&status.queue_depth.to_le_bytes());
    buf.extend_from_slice(&status.bins_ok.to_le_bytes());
    buf.extend_from_slice(&status.bins_total.to_le_bytes());
    buf.extend_from_slice(&status.trigger_period_cmd_ns.to_le_bytes());
    buf.extend_from_slice(&status.trigger_period_meas_ns.to_le_bytes());
    buf.extend_from_slice(&status.pad0.to_le_bytes());
    buf.extend_from_slice(&status.noise_floor_dbm_0.to_le_bytes());
    buf.extend_from_slice(&status.noise_floor_dbm_1.to_le_bytes());
    buf.extend_from_slice(&status.noise_floor_dbm_2.to_le_bytes());
    buf.extend_from_slice(&status.noise_floor_dbm_3.to_le_bytes());
    buf.extend_from_slice(&status.dc_offset_i_0.to_le_bytes());
    buf.extend_from_slice(&status.dc_offset_i_1.to_le_bytes());
    buf.extend_from_slice(&status.dc_offset_i_2.to_le_bytes());
    buf.extend_from_slice(&status.dc_offset_i_3.to_le_bytes());
    buf.extend_from_slice(&status.dc_offset_q_0.to_le_bytes());
    buf.extend_from_slice(&status.dc_offset_q_1.to_le_bytes());
    buf.extend_from_slice(&status.dc_offset_q_2.to_le_bytes());
    buf.extend_from_slice(&status.dc_offset_q_3.to_le_bytes());

    debug_assert_eq!(buf.len(), HEADER_SIZE + STATUS_SIZE);
    buf
}

/// Codifica un suceso de BITE. `text` tiene que coincidir en longitud con
/// `event.text_len` (invariante de quien construye el evento, no del cable).
pub fn encode_bite_event(event: &BiteEvent, text: &str) -> Vec<u8> {
    assert_eq!(
        text.len(),
        event.text_len as usize,
        "text_len no coincide con la longitud del texto"
    );
    assert!(
        text.len() <= u8::MAX as usize,
        "el texto de un bite_event no puede superar 255 B (text_len:u8)"
    );

    let payload_len = BITE_EVENT_SIZE + text.len();
    let mut buf = Vec::with_capacity(HEADER_SIZE + payload_len);
    write_header(&mut buf, MsgType::BiteEvent, payload_len as u32);

    buf.extend_from_slice(&event.event_time_utc_ns.to_le_bytes());
    buf.extend_from_slice(&event.code.to_le_bytes());
    buf.extend_from_slice(&event.value.to_le_bytes());
    buf.push(event.severity);
    buf.push(event.subsystem);
    buf.push(event.text_len);
    buf.push(event.pad0);
    debug_assert_eq!(buf.len(), HEADER_SIZE + BITE_EVENT_SIZE);

    buf.extend_from_slice(text.as_bytes());
    debug_assert_eq!(buf.len(), HEADER_SIZE + payload_len);
    buf
}

/// Codifica un `config_ack`: tamaño fijo, sin carga variable.
pub fn encode_config_ack(ack: &ConfigAck) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE + CONFIG_ACK_SIZE);
    write_header(&mut buf, MsgType::ConfigAck, CONFIG_ACK_SIZE as u32);

    buf.extend_from_slice(&ack.seq.to_le_bytes());
    buf.push(ack.error);
    buf.push(ack.pad0);
    buf.extend_from_slice(&ack.pad1.to_le_bytes());

    debug_assert_eq!(buf.len(), HEADER_SIZE + CONFIG_ACK_SIZE);
    buf
}

/// Codifica un `selftest_result`: tamaño fijo, sin carga variable.
pub fn encode_selftest_result(result: &SelftestResult) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE + SELFTEST_RESULT_SIZE);
    write_header(
        &mut buf,
        MsgType::SelftestResult,
        SELFTEST_RESULT_SIZE as u32,
    );

    buf.extend_from_slice(&result.seq.to_le_bytes());
    buf.extend_from_slice(&result.nonce.to_le_bytes());
    buf.extend_from_slice(&result.capability_flags.to_le_bytes());
    buf.push(result.error);
    buf.push(result.version_major);
    buf.push(result.version_minor);
    buf.push(result.pad0);

    debug_assert_eq!(buf.len(), HEADER_SIZE + SELFTEST_RESULT_SIZE);
    buf
}

/// Codifica un `capabilities`: tamaño fijo, sin carga variable.
pub fn encode_capabilities(caps: &Capabilities) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE + CAPABILITIES_SIZE);
    write_header(&mut buf, MsgType::Capabilities, CAPABILITIES_SIZE as u32);

    buf.extend_from_slice(&caps.moment_mask.to_le_bytes());
    buf.extend_from_slice(&caps.dealias_mask.to_le_bytes());
    buf.extend_from_slice(&caps.estimator_mask.to_le_bytes());
    buf.extend_from_slice(&caps.max_gates.to_le_bytes());
    buf.extend_from_slice(&caps.max_pulses.to_le_bytes());
    buf.push(caps.n_rx_channels);
    buf.push(caps.pad0);

    debug_assert_eq!(buf.len(), HEADER_SIZE + CAPABILITIES_SIZE);
    buf
}

fn parse_common_header(frame: &[u8]) -> Result<(u8, usize), RcpLinkError> {
    if frame.len() < HEADER_SIZE {
        return Err(RcpLinkError::Truncated);
    }
    let magic = u32::from_le_bytes(frame[0..4].try_into().unwrap());
    if magic != MAGIC {
        return Err(RcpLinkError::BadMagic {
            expected: MAGIC,
            got: magic,
        });
    }
    let version_major = frame[4];
    let version_minor = frame[5];
    if version_major != VERSION_MAJOR {
        return Err(RcpLinkError::UnsupportedVersion {
            major: version_major,
            minor: version_minor,
        });
    }
    let msg_type = frame[6];
    let payload_len = u32::from_le_bytes(frame[8..12].try_into().unwrap()) as usize;
    Ok((msg_type, payload_len))
}

/// Decodifica una trama `down` completa (cabecera de 12 B + `payload_len`
/// bytes, tal como los entrega `crate::tcp`). Sólo cubre `config`, `control`
/// y `selftest_request`: son los únicos tres mensajes de sentido `down` del
/// contrato.
pub fn decode_down_frame(frame: &[u8]) -> Result<DownMessage, RcpLinkError> {
    let (msg_type, payload_len) = parse_common_header(frame)?;
    if frame.len() != HEADER_SIZE + payload_len {
        return Err(RcpLinkError::Truncated);
    }
    let body = &frame[HEADER_SIZE..];

    if msg_type == MsgType::Config as u8 {
        Ok(DownMessage::Config(decode_config_body(body)?))
    } else if msg_type == MsgType::Control as u8 {
        Ok(DownMessage::Control(decode_control_body(body)?))
    } else if msg_type == MsgType::SelftestRequest as u8 {
        Ok(DownMessage::SelftestRequest(decode_selftest_request_body(
            body,
        )?))
    } else {
        Err(RcpLinkError::UnexpectedMsgType(msg_type))
    }
}

fn decode_config_body(body: &[u8]) -> Result<Config, RcpLinkError> {
    if body.len() != CONFIG_SIZE {
        return Err(RcpLinkError::Truncated);
    }
    Ok(Config {
        seq: u32::from_le_bytes(body[0..4].try_into().unwrap()),
        moment_mask: u32::from_le_bytes(body[4..8].try_into().unwrap()),
        n_pulses: u16::from_le_bytes(body[8..10].try_into().unwrap()),
        n_gates: u16::from_le_bytes(body[10..12].try_into().unwrap()),
        clutter_filter: body[12],
        dealias_mode: body[13],
        sweep_mode: body[14],
        estimator: body[15],
        rfi_filter: body[16],
        range_dealias: body[17],
        prf_ratio_num: body[18],
        prf_ratio_den: body[19],
        start_range_m: f32::from_le_bytes(body[20..24].try_into().unwrap()),
        gate_spacing_m: f32::from_le_bytes(body[24..28].try_into().unwrap()),
        prf_hz: f32::from_le_bytes(body[28..32].try_into().unwrap()),
        sqi_threshold: f32::from_le_bytes(body[32..36].try_into().unwrap()),
        sig_threshold: f32::from_le_bytes(body[36..40].try_into().unwrap()),
        ccor_threshold: f32::from_le_bytes(body[40..44].try_into().unwrap()),
        log_threshold: f32::from_le_bytes(body[44..48].try_into().unwrap()),
        clutter_width_ms: f32::from_le_bytes(body[48..52].try_into().unwrap()),
        radar_constant_db: f32::from_le_bytes(body[52..56].try_into().unwrap()),
        noise_floor_dbm: f32::from_le_bytes(body[56..60].try_into().unwrap()),
        receiver_gain_db: f32::from_le_bytes(body[60..64].try_into().unwrap()),
        zdr_offset_db: f32::from_le_bytes(body[64..68].try_into().unwrap()),
        phidp_offset_deg: f32::from_le_bytes(body[68..72].try_into().unwrap()),
        antenna_isolation_db: f32::from_le_bytes(body[72..76].try_into().unwrap()),
        wavelength_m: f32::from_le_bytes(body[76..80].try_into().unwrap()),
        polarization_mode: body[80],
        pad0: body[81],
        burst_window_bins: u16::from_le_bytes(body[82..84].try_into().unwrap()),
    })
}

fn decode_control_body(body: &[u8]) -> Result<Control, RcpLinkError> {
    if body.len() != CONTROL_SIZE {
        return Err(RcpLinkError::Truncated);
    }
    Ok(Control {
        seq: u32::from_le_bytes(body[0..4].try_into().unwrap()),
        command: body[4],
        pad0: body[5],
        pad1: u16::from_le_bytes(body[6..8].try_into().unwrap()),
    })
}

fn decode_selftest_request_body(body: &[u8]) -> Result<SelftestRequest, RcpLinkError> {
    if body.len() != SELFTEST_REQUEST_SIZE {
        return Err(RcpLinkError::Truncated);
    }
    Ok(SelftestRequest {
        seq: u32::from_le_bytes(body[0..4].try_into().unwrap()),
        nonce: u32::from_le_bytes(body[4..8].try_into().unwrap()),
    })
}
