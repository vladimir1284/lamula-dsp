//! Empaquetado de celdas de rango generadas en tramas `Ray` del contrato
//! `DRx↔DSP`, uno o varios canales.

use lamula_contract::drx_dsp::{
    MsgType, HEADER_SIZE, MAGIC, RAY_SIZE, VERSION_MAJOR, VERSION_MINOR,
};
use rustfft::num_complex::Complex64;

/// Campos de cabecera de rayo que no dependen de la muestra generada
/// (posición de antena, configuración vigente). `seq`/`trigger_count`/
/// `timestamp_ns` son los del primer pulso; cada trama sucesiva los avanza.
pub struct RayHeaderFields {
    pub seq_start: u32,
    pub timestamp_ns_start: u64,
    pub timestamp_step_ns: u64,
    pub trigger_count_start: u32,
    pub azimuth_raw: u32,
    pub elevation_raw: u32,
    pub prf_div: u32,
    pub pulse_width_idx: u8,
    pub pulse_mode: u8,
    pub cell_mode: u8,
    pub channel_mask: u8,
    pub ray_flags: u8,
}

/// Serializa `channels` (`channels[c][bin]` es la serie temporal del canal
/// `c` en esa celda de rango; todas comparten el mismo número `M` de pulsos)
/// en `M` tramas `Ray` completas (`Header`+`Ray`+payload int16),
/// transponiendo pulso a pulso: la trama `i` lleva la muestra `i` de cada
/// celda de cada canal, tal como las emitiría el DRx real un rayo (un
/// disparo) a la vez. `n_channels = channels.len()`; canal único pasa
/// `&[cells]`.
///
/// `full_scale_counts` fija la convención de cuantización de este crate:
/// una componente I o Q de amplitud unitaria mapea a esa cuenta ADC (ver
/// suposición 3 del plan de implementación — no hay calibración real
/// confirmada todavía).
pub fn pack_rays(
    fields: &RayHeaderFields,
    channels: &[Vec<Vec<Complex64>>],
    full_scale_counts: i16,
) -> Vec<Vec<u8>> {
    assert!(!channels.is_empty(), "hace falta al menos un canal");
    assert!(
        channels.len() <= u8::MAX as usize,
        "demasiados canales para el campo n_channels:u8"
    );
    let bins = channels[0].len();
    assert!(bins > 0, "hace falta al menos una celda de rango");
    assert!(
        bins <= u16::MAX as usize,
        "demasiadas celdas para el campo bins:u16"
    );
    assert!(
        channels.iter().all(|c| c.len() == bins),
        "todos los canales deben compartir el mismo número de celdas"
    );
    let m = channels[0][0].len();
    assert!(
        channels
            .iter()
            .all(|c| c.iter().all(|cell| cell.len() == m)),
        "todas las celdas deben compartir el mismo número de pulsos M"
    );

    let n_channels = channels.len() as u8;
    let payload_len = bins * n_channels as usize * 2 * std::mem::size_of::<i16>();

    (0..m)
        .map(|i| {
            let mut buf = Vec::with_capacity(HEADER_SIZE + RAY_SIZE + payload_len);

            // Header (12 bytes, little-endian).
            buf.extend_from_slice(&MAGIC.to_le_bytes());
            buf.push(VERSION_MAJOR);
            buf.push(VERSION_MINOR);
            buf.push(MsgType::Ray as u8);
            buf.push(0); // flags, reservado en v0.1
            buf.extend_from_slice(&(payload_len as u32).to_le_bytes());

            // Ray (36 bytes, little-endian, mismo orden de campos que el struct).
            buf.extend_from_slice(&fields.seq_start.wrapping_add(i as u32).to_le_bytes());
            let timestamp_ns = fields.timestamp_ns_start + i as u64 * fields.timestamp_step_ns;
            buf.extend_from_slice(&timestamp_ns.to_le_bytes());
            buf.extend_from_slice(
                &fields
                    .trigger_count_start
                    .wrapping_add(i as u32)
                    .to_le_bytes(),
            );
            buf.extend_from_slice(&fields.azimuth_raw.to_le_bytes());
            buf.extend_from_slice(&fields.elevation_raw.to_le_bytes());
            buf.extend_from_slice(&fields.prf_div.to_le_bytes());
            buf.extend_from_slice(&(bins as u16).to_le_bytes());
            buf.push(fields.pulse_width_idx);
            buf.push(fields.pulse_mode);
            buf.push(fields.cell_mode);
            buf.push(n_channels);
            buf.push(fields.channel_mask);
            buf.push(fields.ray_flags);

            // Payload: pares (I,Q) int16, canal más rápido que bin.
            for bin in 0..bins {
                for channel in channels {
                    let sample = channel[bin][i];
                    buf.extend_from_slice(&quantize(sample.re, full_scale_counts).to_le_bytes());
                    buf.extend_from_slice(&quantize(sample.im, full_scale_counts).to_le_bytes());
                }
            }

            debug_assert_eq!(buf.len(), HEADER_SIZE + RAY_SIZE + payload_len);
            buf
        })
        .collect()
}

fn quantize(x: f64, full_scale_counts: i16) -> i16 {
    let scaled = (x * full_scale_counts as f64).round();
    scaled.clamp(i16::MIN as f64, i16::MAX as f64) as i16
}
