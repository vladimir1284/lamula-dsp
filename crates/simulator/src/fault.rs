//! Inyección de fallos sobre tramas ya empaquetadas (`crate::pack_rays`),
//! tal como pide el alcance Stage 1 del simulador (`docs/dsp-plan.md` §3.1:
//! "supports fault injection (malformed frames, dropped rays, encoder
//! glitches, frequency drift) for BITE testing"). Deriva de frecuencia vive
//! en `crate::burst` (es generación de señal, no corrupción de una trama ya
//! generada); este módulo cubre las otras tres categorías, todas mecánicas
//! sobre bytes/listas — sin fórmula propia que oracular.

use lamula_contract::drx_dsp::HEADER_SIZE;

/// Offset absoluto de `azimuth_raw` dentro de una trama `Ray` completa:
/// `Header` (12 B) + `seq` (4 B) + `timestamp_ns` (8 B) + `trigger_count`
/// (4 B) — mismo layout que decodifica
/// `lamula_ingest::wire::decode_ray_frame`. `elevation_raw` sigue
/// inmediatamente después (4 B más).
const RAY_AZIMUTH_OFFSET: usize = HEADER_SIZE + 4 + 8 + 4;

/// Formas de corromper una trama a nivel de bytes, para ejercitar los
/// caminos de error de `lamula_ingest::wire::decode_ray_frame` /
/// `IngestError` sin reconstruir el formato de cable a mano en cada test que
/// lo necesite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFault {
    /// Invierte los bits de `magic` — nunca coincide con `MAGIC`.
    BadMagic,
    /// Sube `version_major` en 1 — versión no soportada.
    UnsupportedVersion,
    /// Trunca la trama a mitad de la cabecera común (antes de los 12 B).
    TruncatedHeader,
    /// Trunca la trama después de la cabecera pero antes de completar el
    /// payload que `payload_len` sigue anunciando.
    TruncatedPayload,
    /// Sube `payload_len` en 4 B sin añadir datos: el lector espera más
    /// muestras de las que realmente vienen.
    PayloadLenTooLarge,
}

/// Aplica `fault` a una copia de `frame`. Nunca falla: siempre devuelve un
/// buffer, corrupto según lo pedido — decidir qué hacer con el error de
/// decodificación es cosa de quien consuma la trama, no de este generador.
///
/// # Panics
/// Si `frame` es más corto que `HEADER_SIZE` (una trama de `pack_rays` nunca
/// lo es).
pub fn corrupt_frame(frame: &[u8], fault: FrameFault) -> Vec<u8> {
    let mut buf = frame.to_vec();
    assert!(
        buf.len() >= HEADER_SIZE,
        "frame más corto que la cabecera común"
    );
    match fault {
        FrameFault::BadMagic => buf[0] ^= 0xFF,
        FrameFault::UnsupportedVersion => buf[4] = buf[4].wrapping_add(1),
        FrameFault::TruncatedHeader => buf.truncate(HEADER_SIZE / 2),
        FrameFault::TruncatedPayload => {
            let keep = HEADER_SIZE + (buf.len() - HEADER_SIZE) / 2;
            buf.truncate(keep);
        }
        FrameFault::PayloadLenTooLarge => {
            let payload_len = u32::from_le_bytes(buf[8..12].try_into().unwrap());
            buf[8..12].copy_from_slice(&(payload_len + 4).to_le_bytes());
        }
    }
    buf
}

/// Descarta las tramas de `frames` cuyo índice esté en `drop_indices` —
/// simula rayos perdidos en el enlace (huecos de `seq` que
/// `lamula_ingest::assembly::RadialAssembler` cuenta en `dropped_pulses`, no
/// fabrica datos para rellenarlos).
pub fn drop_rays(frames: Vec<Vec<u8>>, drop_indices: &[usize]) -> Vec<Vec<u8>> {
    frames
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop_indices.contains(i))
        .map(|(_, f)| f)
        .collect()
}

/// Sobrescribe `azimuth_raw`/`elevation_raw` de una trama ya empaquetada con
/// un valor arbitrario, sin tocar el resto — un glitch puntual del encoder
/// SSI en un solo pulso (salto, valor atascado, o cualquier otro patrón que
/// arme quien llame variando el valor entre tramas sucesivas).
///
/// # Panics
/// Si `frame` es más corto que el offset de `elevation_raw` (una trama de
/// `pack_rays` nunca lo es).
pub fn inject_encoder_glitch(frame: &mut [u8], azimuth_raw: u32, elevation_raw: u32) {
    assert!(
        frame.len() >= RAY_AZIMUTH_OFFSET + 8,
        "frame más corto que el offset de azimuth_raw/elevation_raw"
    );
    frame[RAY_AZIMUTH_OFFSET..RAY_AZIMUTH_OFFSET + 4].copy_from_slice(&azimuth_raw.to_le_bytes());
    frame[RAY_AZIMUTH_OFFSET + 4..RAY_AZIMUTH_OFFSET + 8]
        .copy_from_slice(&elevation_raw.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate_cell, pack_rays, CellParams, RayHeaderFields};
    use lamula_ingest::{decode_ray_frame, IngestError};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    const FULL_SCALE: i16 = i16::MAX;

    fn sample_frames() -> Vec<Vec<u8>> {
        let params = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 0.5,
            wavelength_m: 0.10,
            prt_s: 1.0e-3,
            m: 4,
            noise_floor: 0.0,
        };
        let mut rng = StdRng::seed_from_u64(42);
        let cells = vec![generate_cell(&params, &mut rng)];
        let fields = RayHeaderFields {
            seq_start: 0,
            timestamp_ns_start: 0,
            timestamp_step_ns: 1,
            trigger_count_start: 0,
            azimuth_raw: 100,
            elevation_raw: 5,
            prf_div: 4,
            pulse_width_idx: 0,
            pulse_mode: 0,
            cell_mode: 0,
            channel_mask: 0b0001,
            ray_flags: 0,
        };
        pack_rays(&fields, &[cells], FULL_SCALE)
    }

    #[test]
    fn bad_magic_is_rejected_with_bad_magic_error() {
        let frame = corrupt_frame(&sample_frames()[0], FrameFault::BadMagic);
        assert!(matches!(
            decode_ray_frame(&frame, FULL_SCALE),
            Err(IngestError::BadMagic { .. })
        ));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let frame = corrupt_frame(&sample_frames()[0], FrameFault::UnsupportedVersion);
        assert!(matches!(
            decode_ray_frame(&frame, FULL_SCALE),
            Err(IngestError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn truncated_header_is_rejected_as_truncated() {
        let frame = corrupt_frame(&sample_frames()[0], FrameFault::TruncatedHeader);
        assert!(matches!(
            decode_ray_frame(&frame, FULL_SCALE),
            Err(IngestError::Truncated)
        ));
    }

    #[test]
    fn truncated_payload_is_rejected_as_truncated() {
        let frame = corrupt_frame(&sample_frames()[0], FrameFault::TruncatedPayload);
        assert!(matches!(
            decode_ray_frame(&frame, FULL_SCALE),
            Err(IngestError::Truncated)
        ));
    }

    #[test]
    fn payload_len_too_large_is_rejected_as_truncated() {
        let frame = corrupt_frame(&sample_frames()[0], FrameFault::PayloadLenTooLarge);
        assert!(matches!(
            decode_ray_frame(&frame, FULL_SCALE),
            Err(IngestError::Truncated)
        ));
    }

    #[test]
    fn valid_frame_still_decodes_when_untouched() {
        // Confirma que `sample_frames` en sí es válida — si esto fallara,
        // los tests de arriba estarían pasando por la razón equivocada.
        let frame = sample_frames();
        assert!(decode_ray_frame(&frame[0], FULL_SCALE).is_ok());
    }

    #[test]
    fn drop_rays_removes_only_the_requested_indices() {
        let frames = vec![vec![1u8], vec![2u8], vec![3u8], vec![4u8]];
        let kept = drop_rays(frames, &[1, 3]);
        assert_eq!(kept, vec![vec![1u8], vec![3u8]]);
    }

    #[test]
    fn inject_encoder_glitch_overrides_azimuth_and_elevation_only() {
        let mut frame = sample_frames().remove(0);
        let before = decode_ray_frame(&frame, FULL_SCALE).unwrap();
        inject_encoder_glitch(&mut frame, 999, 42);
        let after = decode_ray_frame(&frame, FULL_SCALE).unwrap();

        assert_eq!(after.azimuth_raw, 999);
        assert_eq!(after.elevation_raw, 42);
        assert_eq!(after.seq, before.seq);
        assert_eq!(after.channels, before.channels);
    }
}
