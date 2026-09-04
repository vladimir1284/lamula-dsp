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
    pub fn split_by_tx_polarization(
        &self,
        series: &[Complex64],
    ) -> (Vec<Complex64>, Vec<Complex64>) {
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

    /// Bits de `channel_mask` puestos, en el mismo orden ascendente que
    /// `channels[]` usa para indexarlos (`drx_dsp::channel`, contrato v0.3):
    /// `channel_bits()[c]` es el bit físico de `channels[c]`.
    fn channel_bits(&self) -> Vec<u8> {
        (0..8u8)
            .map(|i| 1u8 << i)
            .filter(|bit| self.channel_mask & bit != 0)
            .collect()
    }

    /// Índice en `channels[]` del canal marcado por `bit` (una constante de
    /// `drx_dsp::channel`) en `channel_mask`, o `None` si ese canal no está
    /// presente en este radial.
    pub fn channel_index(&self, bit: u8) -> Option<usize> {
        self.channel_bits().iter().position(|&b| b == bit)
    }

    /// Ventana de burst del pulso `pulse_idx` en el canal marcado por `bit`
    /// (típicamente `drx_dsp::channel::TX_BURST_0`/`TX_BURST_1`): los
    /// primeros `window_bins` bins de ese canal en ese pulso — la longitud
    /// que declara `Config::burst_window_bins` del contrato `DSP↔RCP`, el
    /// resto del canal es ruido/silencio (`drx_dsp::channel`, doc del enum).
    /// `None` si el canal no está presente, si `window_bins` es 0, o si el
    /// radial no tiene bins suficientes.
    pub fn burst_window(
        &self,
        bit: u8,
        pulse_idx: usize,
        window_bins: usize,
    ) -> Option<Vec<Complex64>> {
        if window_bins == 0 {
            return None;
        }
        let c = self.channel_index(bit)?;
        let bins = self.channels[c].len().min(window_bins);
        if bins == 0 {
            return None;
        }
        Some(
            (0..bins)
                .map(|bin| self.channels[c][bin][pulse_idx])
                .collect(),
        )
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

        assert_eq!(
            radial.ray_flags,
            vec![ray_flag::TX_POL_V, 0, ray_flag::AZEL_INVALID]
        );
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

    /// Trama con `n_channels` canales de `n_bins` bins cada uno; la muestra
    /// del canal `c`, bin `b` vale `value + 100.0*c as f64 + 10.0*b as f64`,
    /// para poder identificar en los asserts de qué canal/bin salió.
    fn multi_channel_frame(
        seq: u32,
        channel_mask: u8,
        n_channels: usize,
        n_bins: usize,
        value: f64,
    ) -> RawPulseFrame {
        let channels = (0..n_channels)
            .map(|c| {
                (0..n_bins)
                    .map(|b| Complex64::new(value + 100.0 * c as f64 + 10.0 * b as f64, 0.0))
                    .collect()
            })
            .collect();
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
            channel_mask,
            ray_flags: 0,
            channels,
        }
    }

    #[test]
    fn channel_index_maps_ascending_channel_mask_bits_to_channels_positions() {
        // RX_0 (bit 1) + TX_BURST_0 (bit 16): channels[0] es RX_0,
        // channels[1] es TX_BURST_0 — el orden es el de los bits puestos,
        // no el valor del bit.
        let mask = 0b0001_0001;
        let mut assembler = RadialAssembler::new(2);
        assembler
            .feed(multi_channel_frame(0, mask, 2, 1, 1.0))
            .unwrap();
        let radial = assembler
            .feed(multi_channel_frame(1, mask, 2, 1, 2.0))
            .unwrap()
            .expect("2 pulsos juntados");

        assert_eq!(radial.channel_index(1), Some(0)); // RX_0
        assert_eq!(radial.channel_index(16), Some(1)); // TX_BURST_0
        assert_eq!(radial.channel_index(2), None); // RX_1, ausente
    }

    #[test]
    fn burst_window_reads_first_window_bins_of_the_burst_channel_for_one_pulse() {
        // RX_0 (bit 1) + TX_BURST_0 (bit 16), 3 bins: sólo los primeros 2
        // bins del canal de burst son ventana real (`window_bins = 2`).
        let mask = 0b0001_0001;
        let mut assembler = RadialAssembler::new(2);
        assembler
            .feed(multi_channel_frame(0, mask, 2, 3, 1.0))
            .unwrap();
        let radial = assembler
            .feed(multi_channel_frame(1, mask, 2, 3, 2.0))
            .unwrap()
            .expect("2 pulsos juntados");

        // Canal 1 (TX_BURST_0), pulso 1 (value base 2.0): bin0=102.0,
        // bin1=112.0, bin2=122.0 — la ventana de 2 bins corta antes de bin2.
        let window = radial
            .burst_window(16, 1, 2)
            .expect("canal de burst presente");
        assert_eq!(
            window,
            vec![Complex64::new(102.0, 0.0), Complex64::new(112.0, 0.0)]
        );
    }

    #[test]
    fn burst_window_is_none_without_burst_channel_or_zero_window() {
        let mut assembler = RadialAssembler::new(2);
        assembler.feed(frame(0, 1.0)).unwrap();
        let radial = assembler
            .feed(frame(1, 2.0))
            .unwrap()
            .expect("2 pulsos juntados");

        assert_eq!(radial.burst_window(16, 0, 2), None, "no hay canal de burst");
        assert_eq!(
            radial.burst_window(1, 0, 0),
            None,
            "window_bins = 0 no es una ventana"
        );
    }
}
