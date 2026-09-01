//! Decodificación de tramas `Ray` del contrato `DRx↔DSP`
//! (`contract/vendor/drx_dsp_v0_1.rs`): el sentido inverso de
//! `lamula_simulator::pack_rays`, bytes de cable → una muestra por celda de
//! rango, un pulso. Layout exacto verificado contra
//! `crates/simulator/tests/statistical.rs` (offsets de `Ray`, orden
//! canal-más-rápido-que-bin del payload).

use lamula_contract::drx_dsp::{HEADER_SIZE, MAGIC, RAY_SIZE, VERSION_MAJOR};
use rustfft::num_complex::Complex64;

use crate::error::IngestError;

const MSG_TYPE_RAY: u8 = 1;

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
    if frame.len() != HEADER_SIZE + RAY_SIZE + payload_len {
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
    if payload_len != expected_payload_len {
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
