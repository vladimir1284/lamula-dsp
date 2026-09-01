//! Construye un `MomentRay` a partir de un radial ya ensamblado, igual que
//! `crates/rcp-link/tests/vertical_slice.rs` pero con los campos que ahí
//! eran valores fijos de prueba (`start_range_m`, `gate_spacing_m`,
//! `noise_floor_dbm`, `radar_constant_db`) tomados aquí del `config` real
//! aplicado a la sesión, no inventados.
//!
//! Momentos: UZ (sin corregir), V, SQI y SIG — ver el doc-comment de `crate`
//! para por qué no hay más (`pulse_pair_moments` es el único estimador
//! conectado a este binario, y CCOR no se publica porque no hay filtro de
//! clutter conectado tampoco). `acq_time_utc_ns`/`acq_monotonic_ns` usan el
//! mismo `timestamp_ns` del DRx para los dos campos: el contrato `DRx↔DSP`
//! sólo documenta ese campo como "instante del trigger, reloj del DRx", sin
//! confirmar que sea época UTC — la misma simplificación que ya hace el
//! vertical slice.
//!
//! Censura (`docs/algorithms/ruido-y-umbrales.md` §"Umbrales"): se evalúan
//! `sig_threshold`, `sqi_threshold` y `log_threshold` sobre cada celda; si
//! cualquiera de los tres dispara, UZ y V de esa celda se codifican como
//! NaN (`moment_flag::HAS_MISSING` en su bloque, `ray_flag::CENSORED` en el
//! radial) — "se censura el momento publicado, no la muestra de entrada".
//! SQI y SIG en sí NUNCA se censuran por estos umbrales: son el índice que
//! explica por qué la celda se descartó, y ocultarlo sería contradecir la
//! razón de publicarlo (misma página, última frase de esa sección). SIG sí
//! puede salir NaN cuando no está matemáticamente definido (`s_linear <= 0`,
//! ver `lamula_quality::sig_db`) — eso no es censura por umbral, es un valor
//! indefinido, y también marca `HAS_MISSING`.
//! `ccor_threshold` no se aplica: no hay CCOR que evaluar sin filtro de
//! clutter conectado (hueco real, no un umbral que se ignore a propósito).

use lamula_contract::dsp_rcp::{
    data_type, moment_flag, moment_kind, ray_flag, Config, MomentField, MomentRay,
};
use lamula_ingest::{ssi_counts_to_deg, AssembledRadial};
use lamula_moments::{pulse_pair_moments, PulsePairEstimate};
use lamula_noise::{censored_by_sig_threshold, snr_db};
use lamula_quality::{sig_db, sqi};
use lamula_rcp_link::wire::{MomentBlock, UpMessage};

const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// Cantidades derivadas de un `PulsePairEstimate` que la censura y los
/// cuatro momentos publicados necesitan, calculadas una sola vez por celda.
struct GateQuality {
    uz_db: f64,
    sqi_value: Option<f64>,
    sig_value: Option<f64>,
    censored: bool,
}

fn gate_quality(e: &PulsePairEstimate, config: &Config) -> GateQuality {
    let uz_db = if e.s_linear > 0.0 {
        10.0 * e.s_linear.log10()
    } else {
        f64::NEG_INFINITY
    };
    let snr = if e.s_linear > 0.0 {
        snr_db(e.s_linear, e.noise_floor_estimate)
    } else {
        f64::NEG_INFINITY
    };
    // `sqi()` exige r0_raw > 0; sólo falla con una ráfaga exactamente cero,
    // que en datos reales no ocurre (siempre hay algo de ruido del
    // receptor) — se guarda igual para no entrar en pánico ante ese caso
    // degenerado.
    let sqi_value = (e.r0_raw > 0.0).then(|| sqi(e.r0_raw, e.r1_abs));
    let sig_value = sig_db(e.s_linear, e.noise_floor_estimate);

    let censored = censored_by_sig_threshold(snr, config.sig_threshold as f64)
        || sqi_value.map_or(true, |v| v < config.sqi_threshold as f64)
        || uz_db <= config.log_threshold as f64;

    GateQuality {
        uz_db,
        sqi_value,
        sig_value,
        censored,
    }
}

pub fn build_moment_ray(
    radial: &AssembledRadial,
    config: &Config,
    seq: u32,
    first_after_config: bool,
    ssi_counts_per_turn: u32,
    ssi_zero_offset_deg: f64,
) -> UpMessage {
    let wavelength_m = config.wavelength_m as f64;
    let prf_hz = config.prf_hz as f64;
    let prt_s = 1.0 / prf_hz;

    // `pulse_pair_moments` sólo corre sobre el canal 0: ver `capabilities`
    // en `crate::main` (`n_rx_channels: 1`), único canal que este binario
    // declara saber procesar.
    let estimates: Vec<_> = radial.channels[0]
        .iter()
        .map(|series| pulse_pair_moments(series, wavelength_m, prt_s))
        .collect();
    let n_gates = estimates.len() as u16;

    let quality: Vec<GateQuality> = estimates.iter().map(|e| gate_quality(e, config)).collect();
    let any_censored = quality.iter().any(|q| q.censored);

    let uz_values: Vec<f32> = quality
        .iter()
        .map(|q| if q.censored { f32::NAN } else { q.uz_db as f32 })
        .collect();
    let v_values: Vec<f32> = estimates
        .iter()
        .zip(&quality)
        .map(|(e, q)| {
            if q.censored {
                f32::NAN
            } else {
                e.velocity_mps as f32
            }
        })
        .collect();
    // SQI y SIG nunca se censuran por umbral: ver el doc-comment del
    // módulo. Sólo salen NaN cuando la cantidad no está definida
    // (`Option::None` de `gate_quality`).
    let sqi_values: Vec<f32> = quality
        .iter()
        .map(|q| q.sqi_value.map(|v| v as f32).unwrap_or(f32::NAN))
        .collect();
    let sig_values: Vec<f32> = quality
        .iter()
        .map(|q| q.sig_value.map(|v| v as f32).unwrap_or(f32::NAN))
        .collect();

    let az_start_deg =
        ssi_counts_to_deg(radial.azimuth_raw, ssi_counts_per_turn, ssi_zero_offset_deg) as f32;
    let el_start_deg = ssi_counts_to_deg(
        radial.elevation_raw,
        ssi_counts_per_turn,
        ssi_zero_offset_deg,
    ) as f32;

    let mut ray_flags = 0u8;
    if first_after_config {
        ray_flags |= ray_flag::FIRST_AFTER_CONFIG;
    }
    if any_censored {
        ray_flags |= ray_flag::CENSORED;
    }

    let moment_flags = |values: &[f32]| -> u8 {
        if values.iter().any(|v| v.is_nan()) {
            moment_flag::HAS_MISSING
        } else {
            0
        }
    };

    let mut moments = Vec::with_capacity(4);
    if config.moment_mask & (1 << moment_kind::UZ) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::UZ,
                data_type: data_type::F32,
                flags: moment_flags(&uz_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: uz_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::V) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::V,
                data_type: data_type::F32,
                flags: moment_flags(&v_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: v_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::SQI) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::SQI,
                data_type: data_type::F32,
                flags: moment_flags(&sqi_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: sqi_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::SIG) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::SIG,
                data_type: data_type::F32,
                flags: moment_flags(&sig_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: sig_values,
        });
    }

    let ray = MomentRay {
        seq,
        acq_time_utc_ns: radial.timestamp_ns_start,
        acq_monotonic_ns: radial.timestamp_ns_start,
        // Sin controlador de antena en este repo: un solo radial estático
        // por barrido/volumen, todos con índice 0.
        volume_seq: 0,
        sweep_seq: 0,
        ray_index: 0,
        n_gates,
        n_pulses: config.n_pulses,
        bins_valid: n_gates,
        n_moments: moments.len() as u8,
        sweep_mode: config.sweep_mode,
        prf_mode: config.dealias_mode,
        ray_flags,
        pad0: 0,
        az_start_deg,
        az_end_deg: az_start_deg,
        el_start_deg,
        el_end_deg: el_start_deg,
        fixed_angle_deg: el_start_deg,
        start_range_m: config.start_range_m,
        gate_spacing_m: config.gate_spacing_m,
        prf_hz: config.prf_hz,
        nyquist_velocity: (wavelength_m / (4.0 * prt_s)) as f32,
        unambiguous_range_m: (SPEED_OF_LIGHT_M_S / (2.0 * prf_hz)) as f32,
        noise_floor_dbm: config.noise_floor_dbm,
        radar_constant_db: config.radar_constant_db,
    };

    UpMessage::MomentRay { ray, moments }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamula_moments::PulsePairFlag;

    fn config_with_thresholds(
        sig_threshold: f32,
        sqi_threshold: f32,
        log_threshold: f32,
    ) -> Config {
        Config {
            seq: 1,
            moment_mask: 0,
            n_pulses: 64,
            n_gates: 1,
            clutter_filter: 0,
            dealias_mode: 0,
            sweep_mode: 0,
            estimator: 0,
            rfi_filter: 0,
            range_dealias: 0,
            prf_ratio_num: 0,
            prf_ratio_den: 0,
            start_range_m: 0.0,
            gate_spacing_m: 250.0,
            prf_hz: 1000.0,
            sqi_threshold,
            sig_threshold,
            ccor_threshold: 20.0,
            log_threshold,
            clutter_width_ms: 1.0,
            radar_constant_db: 65.0,
            noise_floor_dbm: -108.0,
            receiver_gain_db: 40.0,
            zdr_offset_db: 0.0,
            phidp_offset_deg: 0.0,
            wavelength_m: 0.10,
            pad0: 0,
        }
    }

    fn estimate(
        s_linear: f64,
        r0_raw: f64,
        r1_abs: f64,
        noise_floor_estimate: f64,
    ) -> PulsePairEstimate {
        PulsePairEstimate {
            s_linear,
            r0_raw,
            r1_abs,
            noise_floor_estimate,
            velocity_mps: 3.0,
            spectrum_width_mps: Some(1.0),
            flag: if s_linear > 0.0 {
                PulsePairFlag::Ok
            } else {
                PulsePairFlag::Censored
            },
        }
    }

    #[test]
    fn strong_coherent_signal_is_not_censored() {
        // S=1.0, N=0.01 -> SNR=20dB; r1_abs cerca de r0_raw -> SQI alto;
        // uz_db = 0dB, muy por encima de log_threshold.
        let e = estimate(1.0, 1.01, 0.95, 0.01);
        let config = config_with_thresholds(3.0, 0.4, -10.0);
        let q = gate_quality(&e, &config);
        assert!(!q.censored);
        assert!(q.sqi_value.unwrap() > 0.4);
        assert!(q.sig_value.unwrap() > 3.0);
    }

    #[test]
    fn low_snr_censors_but_still_publishes_sig() {
        // S=0.02, N=0.01 -> SNR=3.01dB, muy por debajo del umbral (10dB).
        let e = estimate(0.02, 1.03, 0.95, 0.01);
        let config = config_with_thresholds(10.0, 0.0, -100.0);
        let q = gate_quality(&e, &config);
        assert!(q.censored, "SNR bajo umbral debería censurar UZ/V");
        assert!(
            q.sig_value.is_some(),
            "SIG no se censura por umbral: sigue publicado aunque censure UZ/V"
        );
    }

    #[test]
    fn low_sqi_censors_even_with_good_snr() {
        // SNR alto (S=1.0, N=0.01) pero r1_abs pequeño frente a r0_raw ->
        // SQI bajo: censura por coherencia, no por SNR.
        let e = estimate(1.0, 1.01, 0.05, 0.01);
        let config = config_with_thresholds(3.0, 0.4, -100.0);
        let q = gate_quality(&e, &config);
        assert!(q.sqi_value.unwrap() < 0.4);
        assert!(q.censored, "SQI bajo umbral debería censurar UZ/V");
        assert!(q.sig_value.is_some(), "SIG sigue publicado");
    }

    #[test]
    fn cell_with_no_detectable_signal_has_undefined_sig() {
        let e = estimate(0.0, 0.01, 0.005, 0.01);
        let config = config_with_thresholds(3.0, 0.4, -10.0);
        let q = gate_quality(&e, &config);
        assert!(q.censored);
        assert!(
            q.sig_value.is_none(),
            "sin señal detectable SIG no está definido, no es 'censurado'"
        );
    }
}
