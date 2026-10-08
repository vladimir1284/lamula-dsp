//! Decodificación de tramas `Ray` del contrato `DRx↔DSP`
//! (`contract/vendor/drx_dsp_v0_1.rs`): el sentido inverso de
//! `lamula_simulator::pack_rays`, bytes de cable → una muestra por celda de
//! rango, un pulso. Layout exacto verificado contra
//! `crates/simulator/tests/statistical.rs` (offsets de `Ray`, orden
//! canal-más-rápido-que-bin del payload).

use lamula_contract::drx_dsp::{
    Afc, Config, MsgType, AFC_SIZE, CONFIG_SIZE, HEADER_SIZE, MAGIC, RAY_SIZE, VERSION_MAJOR,
    VERSION_MINOR,
};
use rustfft::num_complex::Complex64;

use crate::error::IngestError;

const MSG_TYPE_RAY: u8 = 1;

/// Serializa un mensaje `Afc` (`DRx↔DSP`, sentido `down`) como cabecera de
/// 12 B seguida de sus 16 B, sin carga variable — mismo formato que
/// `lamula_simulator::pack_rays` usa para `Ray`, sentido inverso. `apply_at_seq`
/// y `pad0` viajan tal como los trae `afc` (`0` = aplicar ya, por convención
/// del contrato).
pub fn encode_afc_frame(afc: &Afc) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE + AFC_SIZE);
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.push(VERSION_MAJOR);
    buf.push(VERSION_MINOR);
    buf.push(MsgType::Afc as u8);
    buf.push(0); // flags, reservado
    buf.extend_from_slice(&(AFC_SIZE as u32).to_le_bytes());

    buf.extend_from_slice(&afc.nco_phase_inc.to_le_bytes());
    buf.extend_from_slice(&afc.apply_at_seq.to_le_bytes());
    buf.extend_from_slice(&afc.pad0.to_le_bytes());
    debug_assert_eq!(buf.len(), HEADER_SIZE + AFC_SIZE);
    buf
}

/// Serializa un mensaje `Config` (`DRx↔DSP`, sentido `down`, issue #1 ítem 5)
/// como cabecera de 12 B seguida de sus 60 B, sin carga variable. Orden de
/// campos igual que `contract/vendor/drx_dsp_v0_1.rs::Config` (v0.6, dos
/// lotes) — ver `crate::ray::build_drx_config` en `lamula-dsp-service` para
/// cómo se construye este valor a partir de `dsp_rcp::Config`.
pub fn encode_config_frame(cfg: &Config) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE + CONFIG_SIZE);
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.push(VERSION_MAJOR);
    buf.push(VERSION_MINOR);
    buf.push(MsgType::Config as u8);
    buf.push(0); // flags, reservado
    buf.extend_from_slice(&(CONFIG_SIZE as u32).to_le_bytes());

    buf.extend_from_slice(&cfg.seq.to_le_bytes());
    buf.extend_from_slice(&cfg.prf_div_low.to_le_bytes());
    buf.extend_from_slice(&cfg.prf_div_high.to_le_bytes());
    buf.extend_from_slice(&cfg.range_bins_low.to_le_bytes());
    buf.extend_from_slice(&cfg.range_bins_high.to_le_bytes());
    buf.extend_from_slice(&cfg.n_pulses_low.to_le_bytes());
    buf.extend_from_slice(&cfg.n_pulses_high.to_le_bytes());
    buf.push(cfg.pulse_width_idx_low);
    buf.push(cfg.pulse_width_idx_high);
    buf.push(cfg.pulse_mode);
    buf.push(cfg.cell_mode);
    buf.push(cfg.channel_mask);
    buf.push(cfg.scan_mode);
    buf.push(cfg.pad0);
    buf.push(cfg.pad1);
    buf.extend_from_slice(&cfg.trigger_delay_0.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_delay_1.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_delay_2.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_delay_3.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_width_0.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_width_1.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_width_2.to_le_bytes());
    buf.extend_from_slice(&cfg.trigger_width_3.to_le_bytes());
    debug_assert_eq!(buf.len(), HEADER_SIZE + CONFIG_SIZE);
    buf
}

#[cfg(test)]
mod config_encode_tests {
    use super::*;
    use lamula_contract::drx_dsp::channel;

    #[test]
    fn encode_config_frame_matches_header_and_payload_layout() {
        let cfg = Config {
            seq: 7,
            prf_div_low: 40,
            prf_div_high: 50,
            range_bins_low: 1000,
            range_bins_high: 1100,
            n_pulses_low: 64,
            n_pulses_high: 32,
            pulse_width_idx_low: 2,
            pulse_width_idx_high: 3,
            pulse_mode: 0,
            cell_mode: 1,
            channel_mask: channel::RX_0 | channel::RX_2,
            scan_mode: 1,
            pad0: 0,
            pad1: 0,
            trigger_delay_0: 100,
            trigger_delay_1: 200,
            trigger_delay_2: 300,
            trigger_delay_3: 400,
            trigger_width_0: 10,
            trigger_width_1: 20,
            trigger_width_2: 30,
            trigger_width_3: 40,
        };
        let frame = encode_config_frame(&cfg);
        assert_eq!(frame.len(), HEADER_SIZE + CONFIG_SIZE);
        assert_eq!(&frame[0..4], &MAGIC.to_le_bytes());
        assert_eq!(frame[6], MsgType::Config as u8);
        assert_eq!(
            u32::from_le_bytes(frame[8..12].try_into().unwrap()),
            CONFIG_SIZE as u32
        );
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(frame[16..20].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(frame[20..24].try_into().unwrap()), 50);
        assert_eq!(u16::from_le_bytes(frame[24..26].try_into().unwrap()), 1000);
        assert_eq!(u16::from_le_bytes(frame[26..28].try_into().unwrap()), 1100);
        assert_eq!(u16::from_le_bytes(frame[28..30].try_into().unwrap()), 64);
        assert_eq!(u16::from_le_bytes(frame[30..32].try_into().unwrap()), 32);
        assert_eq!(frame[32], 2);
        assert_eq!(frame[33], 3);
        assert_eq!(frame[34], 0);
        assert_eq!(frame[35], 1);
        assert_eq!(frame[36], channel::RX_0 | channel::RX_2);
        assert_eq!(frame[37], 1);
        assert_eq!(frame[38], 0);
        assert_eq!(frame[39], 0);
        assert_eq!(u32::from_le_bytes(frame[40..44].try_into().unwrap()), 100);
        assert_eq!(u32::from_le_bytes(frame[52..56].try_into().unwrap()), 400);
        assert_eq!(u32::from_le_bytes(frame[56..60].try_into().unwrap()), 10);
        assert_eq!(u32::from_le_bytes(frame[68..72].try_into().unwrap()), 40);
    }
}

#[cfg(test)]
mod afc_encode_tests {
    use super::*;

    #[test]
    fn encode_afc_frame_matches_header_and_payload_layout() {
        let afc = Afc {
            nco_phase_inc: 0x0102_0304_0506_0708,
            apply_at_seq: 42,
            pad0: 0,
        };
        let frame = encode_afc_frame(&afc);
        let nco_phase_inc = afc.nco_phase_inc; // copia local: `Afc` es `packed`
        let apply_at_seq = afc.apply_at_seq;
        assert_eq!(frame.len(), HEADER_SIZE + AFC_SIZE);
        assert_eq!(&frame[0..4], &MAGIC.to_le_bytes());
        assert_eq!(frame[6], MsgType::Afc as u8);
        assert_eq!(
            u32::from_le_bytes(frame[8..12].try_into().unwrap()),
            AFC_SIZE as u32
        );
        assert_eq!(
            u64::from_le_bytes(frame[12..20].try_into().unwrap()),
            nco_phase_inc
        );
        assert_eq!(
            u32::from_le_bytes(frame[20..24].try_into().unwrap()),
            apply_at_seq
        );
    }
}

/// Una trama `Ray` decodificada: un pulso, todas las celdas y canales.
/// `channels[c][bin]` es la muestra compleja de ese canal en esa celda para
/// este pulso — forma `[canal][bin]`, sin dimensión de pulso todavía; eso lo
/// añade `crate::assembly::RadialAssembler` juntando varias `RawPulseFrame`.
#[derive(Debug, Clone, PartialEq)]
pub struct RawPulseFrame {
    pub seq: u32,
    pub timestamp_ns: u64,
    pub trigger_count: u32,
    pub azimuth_raw: u32,
    pub elevation_raw: u32,
    pub prf_div: u32,
    pub pulse_width_idx: u8,
    pub pulse_mode: u8,
    pub cell_mode: u8,
    pub channel_mask: u8,
    pub ray_flags: u8,
    pub channels: Vec<Vec<Complex64>>,
}

/// Decodifica una trama completa (`Header`+`Ray`+payload) tal como la
/// devuelve un elemento de `lamula_simulator::pack_rays`.
///
/// `full_scale_counts` tiene que ser el mismo valor que se usó para
/// cuantizar en el otro extremo (`pack_rays`) — no es parte del contrato de
/// cable, es una convención de cuantización sin calibración real confirmada
/// todavía (ver el doc-comment de `pack_rays` en
/// `crates/simulator/src/ray.rs`).
pub fn decode_ray_frame(
    frame: &[u8],
    full_scale_counts: i16,
) -> Result<RawPulseFrame, IngestError> {
    if frame.len() < HEADER_SIZE {
        return Err(IngestError::Truncated);
    }

    let magic = u32::from_le_bytes(frame[0..4].try_into().unwrap());
    if magic != MAGIC {
        return Err(IngestError::BadMagic {
            expected: MAGIC,
            got: magic,
        });
    }
    let version_major = frame[4];
    let version_minor = frame[5];
    if version_major != VERSION_MAJOR {
        return Err(IngestError::UnsupportedVersion {
            major: version_major,
            minor: version_minor,
        });
    }
    let msg_type = frame[6];
    if msg_type != MSG_TYPE_RAY {
        return Err(IngestError::UnexpectedMsgType(msg_type));
    }
    let payload_len = u32::from_le_bytes(frame[8..12].try_into().unwrap()) as usize;
    // `payload_len` cuenta lo que sigue a la cabecera de 12 B: el struct `ray`
    // de `RAY_SIZE` más la carga de I/Q (esquema, doc de `header.payload_len`).
    if frame.len() != HEADER_SIZE + payload_len || payload_len < RAY_SIZE {
        return Err(IngestError::Truncated);
    }

    let ray = &frame[HEADER_SIZE..HEADER_SIZE + RAY_SIZE];
    let seq = u32::from_le_bytes(ray[0..4].try_into().unwrap());
    let timestamp_ns = u64::from_le_bytes(ray[4..12].try_into().unwrap());
    let trigger_count = u32::from_le_bytes(ray[12..16].try_into().unwrap());
    let azimuth_raw = u32::from_le_bytes(ray[16..20].try_into().unwrap());
    let elevation_raw = u32::from_le_bytes(ray[20..24].try_into().unwrap());
    let prf_div = u32::from_le_bytes(ray[24..28].try_into().unwrap());
    let bins = u16::from_le_bytes(ray[28..30].try_into().unwrap()) as usize;
    let pulse_width_idx = ray[30];
    let pulse_mode = ray[31];
    let cell_mode = ray[32];
    let n_channels = ray[33] as usize;
    let channel_mask = ray[34];
    let ray_flags = ray[35];

    let expected_payload_len = bins * n_channels * 2 * std::mem::size_of::<i16>();
    if payload_len != RAY_SIZE + expected_payload_len {
        return Err(IngestError::Truncated);
    }

    let payload = &frame[HEADER_SIZE + RAY_SIZE..];
    let mut channels: Vec<Vec<Complex64>> =
        (0..n_channels).map(|_| Vec::with_capacity(bins)).collect();
    for bin in 0..bins {
        for (c, channel) in channels.iter_mut().enumerate() {
            let base = (bin * n_channels + c) * 4;
            let i = i16::from_le_bytes(payload[base..base + 2].try_into().unwrap());
            let q = i16::from_le_bytes(payload[base + 2..base + 4].try_into().unwrap());
            channel.push(dequantize(i, q, full_scale_counts));
        }
    }

    Ok(RawPulseFrame {
        seq,
        timestamp_ns,
        trigger_count,
        azimuth_raw,
        elevation_raw,
        prf_div,
        pulse_width_idx,
        pulse_mode,
        cell_mode,
        channel_mask,
        ray_flags,
        channels,
    })
}

fn dequantize(i: i16, q: i16, full_scale_counts: i16) -> Complex64 {
    let scale = full_scale_counts as f64;
    Complex64::new(i as f64 / scale, q as f64 / scale)
}
