//! Ensamblado de radial: junta `n_pulses` tramas `RawPulseFrame`
//! consecutivas (mismo `seq` creciente) en la serie temporal por canal/celda
//! que ya esperan los crates de algoritmo (`&[Complex64]`, ver
//! `lamula_moments::pulse_pair_moments`). Es el inverso de
//! `lamula_simulator::pack_rays`: `pack_rays` recibe
//! `channels: &[Vec<Vec<Complex64>>]` de forma `[canal][bin][pulso]` y emite
//! `M` tramas; `RadialAssembler` hace el camino contrario.
//!
//! **Límite documentado, no resuelto aquí:** el radial se cierra por conteo
//! de pulsos llegados (`n_pulses`), no por un marcador explícito de fin de
//! ráfaga — el contrato `DRx↔DSP` v0.1 no tiene uno (`ray_flags` sólo marca
//! `FIRST_AFTER_CONFIG`, no "último pulso de la ráfaga"). Si se pierde un
//! pulso, el radial completado igual junta `n_pulses` muestras, pero la
//! última puede pertenecer ya a la ráfaga siguiente; `dropped_pulses` deja
//! constancia del hueco para que una capa de BITE futura decida censurar.
//! Tampoco convierte `azimuth_raw`/`elevation_raw` (cuentas de encoder SSI) a
//! grados — eso lo hace [`crate::angle::ssi_counts_to_deg`], por separado,
//! con la resolución y el offset de cero del encoder que quien ensambla el
//! `MomentRay` deba conocer.
//!
//! Asume entrega en orden: TCP lo garantiza; UDP en una LAN conmutada
//! punto a punto no reordena en la práctica, pero esta versión no detecta
//! reordenado, sólo pérdida (huecos de `seq` creciente).

use lamula_contract::drx_dsp::ray_flag;
use rustfft::num_complex::Complex64;

use crate::error::IngestError;
use crate::wire::RawPulseFrame;

/// Un radial ensamblado: `channels[c][bin]` es la serie temporal completa
/// (`n_pulses` muestras) de ese canal en esa celda de rango, lista para
/// pasar tal cual a un estimador de momentos.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledRadial {
    pub seq_start: u32,
    pub timestamp_ns_start: u64,
    pub trigger_count_start: u32,
    pub azimuth_raw: u32,
    pub elevation_raw: u32,
    pub prf_div: u32,
    pub pulse_width_idx: u8,
    pub pulse_mode: u8,
    pub cell_mode: u8,
    pub channel_mask: u8,
    pub channels: Vec<Vec<Vec<Complex64>>>,
    /// `ray_flags` de cada pulso, en el mismo orden que las muestras de
    /// `channels[c][bin]` — un byte por pulso, `n_pulses` de largo. Incluye
    /// el bit `ray_flag::TX_POL_V`, que [`split_by_tx_polarization`] usa
    /// para separar una serie en subseries H/V.
    ///
    /// [`split_by_tx_polarization`]: AssembledRadial::split_by_tx_polarization
    pub ray_flags: Vec<u8>,
    /// Pulsos que faltaron en la ráfaga (huecos de `seq`), no inventados.
    pub dropped_pulses: u32,
}

impl AssembledRadial {
    /// Separa una serie de pulsos de un canal (`channels[c][bin]`, o
    /// cualquier serie alineada con `ray_flags`) en dos subseries por
    /// polarización de transmisión, preservando el orden de llegada: pulsos
    /// transmitidos en H (bit `ray_flag::TX_POL_V` a cero) y pulsos
    /// transmitidos en V (bit a uno).
    ///
    /// Sólo tiene sentido con polarización alternante H/V. En canal único o
    /// simultánea (STAR) el bit vale 0 en todos los pulsos del `DRx↔DSP` v0.2,
    /// así que la subserie V sale vacía.
    pub fn split_by_tx_polarization(&self, series: &[Complex64]) -> (Vec<Complex64>, Vec<Complex64>) {
        assert_eq!(
            series.len(),
            self.ray_flags.len(),
            "serie y ray_flags deben tener la misma longitud de pulsos"
        );
        let mut h = Vec::new();
        let mut v = Vec::new();
        for (&sample, &flags) in series.iter().zip(&self.ray_flags) {
            if flags & ray_flag::TX_POL_V != 0 {
                v.push(sample);
            } else {
                h.push(sample);
            }
        }
        (h, v)
    }
}

/// Junta tramas de un pulso en radiales de `n_pulses` pulsos. Un assembler
/// por ráfaga en curso; `feed` devuelve `Some` cuando el radial se completa.
pub struct RadialAssembler {
    n_pulses: u16,
    pending: Vec<RawPulseFrame>,
    last_seq: Option<u32>,
    dropped_pulses: u32,
}

impl RadialAssembler {
    /// El ensamblador ya recibe tramas decodificadas (`RawPulseFrame`, ya
    /// deshecha la cuantización) de `crate::wire::decode_ray_frame` — no
    /// necesita `full_scale_counts` porque no vuelve a tocar los bytes de
    /// cable.
    pub fn new(n_pulses: u16) -> Self {
        assert!(n_pulses >= 2, "hacen falta al menos dos pulsos por radial");
        Self {
            n_pulses,
            pending: Vec::with_capacity(n_pulses as usize),
            last_seq: None,
            dropped_pulses: 0,
        }
    }

    /// Cuenta de pulsos perdidos detectados hasta ahora en la ráfaga en
    /// curso (se reinicia al completar un radial).
    pub fn dropped_pulses(&self) -> u32 {
        self.dropped_pulses
    }

    /// Alimenta una trama ya decodificada (ver `crate::wire::decode_ray_frame`).
    pub fn feed(&mut self, frame: RawPulseFrame) -> Result<Option<AssembledRadial>, IngestError> {
        if let Some(first) = self.pending.first() {
            let bins = first.channels.first().map_or(0, Vec::len);
            let frame_bins = frame.channels.first().map_or(0, Vec::len);
            if frame.channels.len() != first.channels.len()
                || frame_bins != bins
                || frame.prf_div != first.prf_div
                || frame.channel_mask != first.channel_mask
            {
                return Err(IngestError::ChannelMismatch);
            }
        }

        if let Some(last) = self.last_seq {
            let gap = frame.seq.wrapping_sub(last);
            if gap == 0 {
                return Err(IngestError::DuplicateSeq);
            }
            self.dropped_pulses += gap - 1;
        }
        self.last_seq = Some(frame.seq);
        self.pending.push(frame);

        if self.pending.len() < self.n_pulses as usize {
            return Ok(None);
        }

        Ok(Some(self.finish()))
    }

    /// Descarta cualquier acumulación en curso sin producir un radial.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.last_seq = None;
        self.dropped_pulses = 0;
    }

    fn finish(&mut self) -> AssembledRadial {
        let frames = std::mem::take(&mut self.pending);
        self.last_seq = None;
        let dropped_pulses = std::mem::take(&mut self.dropped_pulses);

        let ray_flags: Vec<u8> = frames.iter().map(|f| f.ray_flags).collect();

        let first = &frames[0];
        let n_channels = first.channels.len();
        let bins = first.channels.first().map_or(0, Vec::len);
        let mut channels: Vec<Vec<Vec<Complex64>>> = (0..n_channels)
            .map(|_| {
                (0..bins)
                    .map(|_| Vec::with_capacity(frames.len()))
                    .collect()
            })
            .collect();
        for frame in &frames {
            for (c, ch) in frame.channels.iter().enumerate() {
                for (bin, sample) in ch.iter().enumerate() {
                    channels[c][bin].push(*sample);
                }
            }
        }

        AssembledRadial {
            seq_start: first.seq,
            timestamp_ns_start: first.timestamp_ns,
            trigger_count_start: first.trigger_count,
            azimuth_raw: first.azimuth_raw,
            elevation_raw: first.elevation_raw,
            prf_div: first.prf_div,
            pulse_width_idx: first.pulse_width_idx,
            pulse_mode: first.pulse_mode,
            cell_mode: first.cell_mode,
            channel_mask: first.channel_mask,
            channels,
            ray_flags,
            dropped_pulses,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(seq: u32, value: f64) -> RawPulseFrame {
        frame_with_flags(seq, value, 0)
    }

    fn frame_with_flags(seq: u32, value: f64, ray_flags: u8) -> RawPulseFrame {
        RawPulseFrame {
            seq,
            timestamp_ns: seq as u64,
            trigger_count: seq,
            azimuth_raw: 0,
            elevation_raw: 0,
            prf_div: 4,
            pulse_width_idx: 0,
            pulse_mode: 0,
            cell_mode: 0,
            channel_mask: 0b0001,
            ray_flags,
            channels: vec![vec![Complex64::new(value, 0.0)]],
        }
    }

    #[test]
    fn gap_in_seq_is_counted_as_dropped_pulses_not_fabricated() {
        let mut assembler = RadialAssembler::new(3);

        assert_eq!(assembler.feed(frame(0, 1.0)).unwrap(), None);
        assert_eq!(assembler.dropped_pulses(), 0);
        assert_eq!(assembler.feed(frame(1, 2.0)).unwrap(), None);
        assert_eq!(assembler.dropped_pulses(), 0);
        // Salta seq=2: un pulso perdido.
        let radial = assembler
            .feed(frame(3, 3.0))
            .unwrap()
            .expect("3 pulsos juntados");

        assert_eq!(radial.dropped_pulses, 1);
        assert_eq!(
            radial.channels[0][0].len(),
            3,
            "no se inventa la muestra faltante"
        );
        assert_eq!(
            radial.channels[0][0],
            vec![
                Complex64::new(1.0, 0.0),
                Complex64::new(2.0, 0.0),
                Complex64::new(3.0, 0.0)
            ]
        );
    }

    #[test]
    fn duplicate_seq_is_rejected() {
        let mut assembler = RadialAssembler::new(2);
        assembler.feed(frame(5, 1.0)).unwrap();
        assert!(matches!(
            assembler.feed(frame(5, 2.0)),
            Err(IngestError::DuplicateSeq)
        ));
    }

    #[test]
    fn ray_flags_are_carried_into_the_assembled_radial_in_pulse_order() {
        let mut assembler = RadialAssembler::new(3);
        assembler
            .feed(frame_with_flags(0, 1.0, ray_flag::TX_POL_V))
            .unwrap();
        assembler.feed(frame_with_flags(1, 2.0, 0)).unwrap();
        let radial = assembler
            .feed(frame_with_flags(2, 3.0, ray_flag::AZEL_INVALID))
            .unwrap()
            .expect("3 pulsos juntados");

        assert_eq!(radial.ray_flags, vec![ray_flag::TX_POL_V, 0, ray_flag::AZEL_INVALID]);
    }

    #[test]
    fn split_by_tx_polarization_separates_h_and_v_pulses_in_arrival_order() {
        let mut assembler = RadialAssembler::new(4);
        assembler
            .feed(frame_with_flags(0, 1.0, 0)) // H
            .unwrap();
        assembler
            .feed(frame_with_flags(1, 2.0, ray_flag::TX_POL_V)) // V
            .unwrap();
        assembler
            .feed(frame_with_flags(2, 3.0, 0)) // H
            .unwrap();
        let radial = assembler
            .feed(frame_with_flags(3, 4.0, ray_flag::TX_POL_V)) // V
            .unwrap()
            .expect("4 pulsos juntados");

        let (h, v) = radial.split_by_tx_polarization(&radial.channels[0][0]);
        assert_eq!(h, vec![Complex64::new(1.0, 0.0), Complex64::new(3.0, 0.0)]);
        assert_eq!(v, vec![Complex64::new(2.0, 0.0), Complex64::new(4.0, 0.0)]);
    }

    #[test]
    fn split_by_tx_polarization_is_a_noop_when_tx_pol_v_is_never_set() {
        let mut assembler = RadialAssembler::new(2);
        assembler.feed(frame(0, 1.0)).unwrap();
        let radial = assembler
            .feed(frame(1, 2.0))
            .unwrap()
            .expect("2 pulsos juntados");

        let (h, v) = radial.split_by_tx_polarization(&radial.channels[0][0]);
        assert_eq!(h, radial.channels[0][0]);
        assert!(v.is_empty());
    }
}
