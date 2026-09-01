//! Validación de un `config` entrante contra las `capabilities` vigentes del
//! DSP, devolviendo uno de los códigos de `lamula_contract::dsp_rcp::error`
//! que `ConfigAck.error` espera.
//!
//! Función pura a propósito: no conoce fase (`setup`/`running`/`fault`) ni
//! historial de configuración — `not_in_setup_phase` y `not_configured` son
//! del resorte de quien lleve esa máquina de estados (fuera de este crate,
//! ver `crate` doc), no de esta validación de datos. Tampoco inventa códigos
//! de rechazo nuevos: el contrato v0.1 no tiene uno para `sweep_mode` o
//! `clutter_filter` fuera de tabla (a diferencia de `DRx↔DSP`, que sí tiene
//! `SCAN_MODE_INVALID`/`CELL_MODE_INVALID`); ese es un hueco real del
//! contrato, no algo que este módulo pueda tapar sin inventar semántica.
//!
//! Cobertura honesta de `threshold_out_of_range`: el único umbral con rango
//! documentado en este repositorio es `sqi_threshold` — SQI se define "0 a 1"
//! en la enumeración `moment_kind` del propio esquema. `sig_threshold`,
//! `ccor_threshold` y `log_threshold` no tienen rango publicado en ningún
//! sitio (`docs/algorithms/ruido-y-umbrales.md` los describe cualitativamente,
//! sin cifras) — validar un número ahí sería inventar una regla de negocio,
//! así que se dejan sin comprobar hasta que exista una referencia real.
//! `clutter_width_ms`/`wavelength_m`/`prf_hz`/`gate_spacing_m` sí llevan una
//! comprobación: no de rango de negocio, sino de que no violen su propio
//! significado físico (una anchura o una longitud de onda no puede ser
//! negativa).

use lamula_contract::dsp_rcp::{dealias_mode, error, Capabilities, Config};

/// Velocidad de la luz en el vacío, m/s. La atmósfera la reduce en partes por
/// millón (índice de refracción ≈ 1.0003); a la escala de este margen de
/// seguridad la diferencia no importa.
const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// Comprueba `config` contra `caps` y devuelve el primer código de rechazo
/// que aplique, en el mismo orden en que aparecen en la enumeración `error`
/// del esquema (más específico primero: capacidades antes que rangos, para
/// que el operador vea "esta compilación no sabe hacer X" antes que "el
/// número que pediste es absurdo" cuando ambas cosas fallan a la vez).
pub fn validate_config(config: &Config, caps: &Capabilities) -> Result<(), u8> {
    if config.moment_mask & !caps.moment_mask != 0 {
        return Err(error::MOMENT_UNSUPPORTED);
    }

    if config.dealias_mode != dealias_mode::NONE {
        match mode_bit(config.dealias_mode) {
            Some(bit) if caps.dealias_mask & bit != 0 => {}
            _ => return Err(error::DEALIAS_UNSUPPORTED),
        }
    }

    match mode_bit(config.estimator) {
        Some(bit) if caps.estimator_mask & bit != 0 => {}
        _ => return Err(error::ESTIMATOR_UNSUPPORTED),
    }

    if config.n_gates as u32 > caps.max_gates {
        return Err(error::GATE_COUNT_ILLEGAL);
    }

    let sqi_threshold = config.sqi_threshold;
    if !(0.0..=1.0).contains(&sqi_threshold) {
        return Err(error::THRESHOLD_OUT_OF_RANGE);
    }
    if !config.clutter_width_ms.is_finite() || config.clutter_width_ms < 0.0 {
        return Err(error::THRESHOLD_OUT_OF_RANGE);
    }
    if !config.wavelength_m.is_finite() || config.wavelength_m <= 0.0 {
        return Err(error::THRESHOLD_OUT_OF_RANGE);
    }
    if !config.gate_spacing_m.is_finite() || config.gate_spacing_m <= 0.0 {
        return Err(error::THRESHOLD_OUT_OF_RANGE);
    }
    if !config.start_range_m.is_finite() || config.start_range_m < 0.0 {
        return Err(error::THRESHOLD_OUT_OF_RANGE);
    }

    if !config.prf_hz.is_finite() || config.prf_hz <= 0.0 {
        return Err(error::PRF_RANGE_ILLEGAL);
    }
    // Proxy físico de `prf_range_illegal`, NO el texto literal de la
    // decisión D-09 del proyecto lamula-drx externo — esa decisión no está
    // vendorizada en este repositorio, sólo referenciada por nombre en el
    // esquema. Lo que sí es verificable aquí es la física que el propio
    // contrato documenta en el campo `unambiguous_range_m`: rango no
    // ambiguo = c/(2·PRF). Si D-09 añade márgenes u otras excepciones
    // (p.ej. para split-cut o recuperación de trip), este cálculo no las
    // conoce y hay que revisarlo contra D-09 real antes de confiar en él
    // para comisionar hardware.
    let requested_extent_m =
        config.start_range_m as f64 + config.n_gates as f64 * config.gate_spacing_m as f64;
    let unambiguous_range_m = SPEED_OF_LIGHT_M_S / (2.0 * config.prf_hz as f64);
    if requested_extent_m > unambiguous_range_m {
        return Err(error::PRF_RANGE_ILLEGAL);
    }

    Ok(())
}

/// `1 << value` protegido contra `value >= 32`, donde un `u8` que codifica un
/// modo fuera de tabla no puede tener bit de capacidad — `None` se trata
/// igual que "bit no presente" en las dos llamadas de arriba.
fn mode_bit(value: u8) -> Option<u32> {
    1u32.checked_shl(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamula_contract::dsp_rcp::{clutter_filter, estimator, moment_kind, sweep_mode};

    fn permissive_config() -> Config {
        Config {
            seq: 1,
            moment_mask: 1 << moment_kind::UZ | 1 << moment_kind::V,
            n_pulses: 64,
            n_gates: 500,
            clutter_filter: clutter_filter::NONE,
            dealias_mode: dealias_mode::NONE,
            sweep_mode: sweep_mode::PPI,
            estimator: estimator::PULSE_PAIR,
            rfi_filter: 0,
            range_dealias: 0,
            prf_ratio_num: 0,
            prf_ratio_den: 0,
            start_range_m: 0.0,
            gate_spacing_m: 250.0,
            prf_hz: 300.0, // c/(2*300) ≈ 500 km, sobra para 500*250 m = 125 km
            sqi_threshold: 0.4,
            sig_threshold: 3.0,
            ccor_threshold: 20.0,
            log_threshold: -10.0,
            clutter_width_ms: 1.0,
            radar_constant_db: 65.0,
            noise_floor_dbm: -108.0,
            receiver_gain_db: 40.0,
            zdr_offset_db: 0.0,
            phidp_offset_deg: 0.0,
            wavelength_m: 0.1,
            pad0: 0,
        }
    }

    fn full_capabilities() -> Capabilities {
        Capabilities {
            moment_mask: 0xFFFF,
            dealias_mask: 1 << dealias_mode::NONE
                | 1 << dealias_mode::DUAL_PRF
                | 1 << dealias_mode::STAGGERED_PRT,
            estimator_mask: 1 << estimator::PULSE_PAIR | 1 << estimator::SPECTRAL,
            max_gates: 2000,
            max_pulses: 128,
            n_rx_channels: 2,
            pad0: 0,
        }
    }

    #[test]
    fn permissive_config_is_accepted() {
        assert_eq!(
            validate_config(&permissive_config(), &full_capabilities()),
            Ok(())
        );
    }

    #[test]
    fn rejects_moment_not_in_capability_mask() {
        let mut caps = full_capabilities();
        caps.moment_mask = 1 << moment_kind::UZ; // sin V
        assert_eq!(
            validate_config(&permissive_config(), &caps),
            Err(error::MOMENT_UNSUPPORTED)
        );
    }

    #[test]
    fn dealias_none_is_always_accepted_even_with_empty_capability_mask() {
        let mut caps = full_capabilities();
        caps.dealias_mask = 0;
        let cfg = permissive_config(); // dealias_mode = NONE
        assert_eq!(validate_config(&cfg, &caps), Ok(()));
    }

    #[test]
    fn rejects_dealias_mode_not_in_capability_mask() {
        let mut caps = full_capabilities();
        caps.dealias_mask = 1 << dealias_mode::NONE; // sin dual_prf
        let mut cfg = permissive_config();
        cfg.dealias_mode = dealias_mode::DUAL_PRF;
        assert_eq!(
            validate_config(&cfg, &caps),
            Err(error::DEALIAS_UNSUPPORTED)
        );
    }

    #[test]
    fn rejects_estimator_not_in_capability_mask() {
        let mut caps = full_capabilities();
        caps.estimator_mask = 1 << estimator::PULSE_PAIR; // sin espectral
        let mut cfg = permissive_config();
        cfg.estimator = estimator::SPECTRAL;
        assert_eq!(
            validate_config(&cfg, &caps),
            Err(error::ESTIMATOR_UNSUPPORTED)
        );
    }

    #[test]
    fn rejects_out_of_table_estimator_without_panicking_on_shift() {
        let caps = full_capabilities();
        let mut cfg = permissive_config();
        cfg.estimator = 200; // fuera de la enumeración `estimator`
        assert_eq!(
            validate_config(&cfg, &caps),
            Err(error::ESTIMATOR_UNSUPPORTED)
        );
    }

    #[test]
    fn rejects_n_gates_above_max_gates() {
        let mut caps = full_capabilities();
        caps.max_gates = 100;
        let cfg = permissive_config(); // n_gates = 500
        assert_eq!(validate_config(&cfg, &caps), Err(error::GATE_COUNT_ILLEGAL));
    }

    #[test]
    fn rejects_sqi_threshold_out_of_zero_one() {
        let caps = full_capabilities();
        let mut cfg = permissive_config();
        cfg.sqi_threshold = 1.5;
        assert_eq!(
            validate_config(&cfg, &caps),
            Err(error::THRESHOLD_OUT_OF_RANGE)
        );
        cfg.sqi_threshold = -0.1;
        assert_eq!(
            validate_config(&cfg, &caps),
            Err(error::THRESHOLD_OUT_OF_RANGE)
        );
        cfg.sqi_threshold = f32::NAN;
        assert_eq!(
            validate_config(&cfg, &caps),
            Err(error::THRESHOLD_OUT_OF_RANGE)
        );
    }

    #[test]
    fn rejects_negative_clutter_width() {
        let caps = full_capabilities();
        let mut cfg = permissive_config();
        cfg.clutter_width_ms = -1.0;
        assert_eq!(
            validate_config(&cfg, &caps),
            Err(error::THRESHOLD_OUT_OF_RANGE)
        );
    }

    #[test]
    fn rejects_non_positive_wavelength() {
        let caps = full_capabilities();
        let mut cfg = permissive_config();
        cfg.wavelength_m = 0.0;
        assert_eq!(
            validate_config(&cfg, &caps),
            Err(error::THRESHOLD_OUT_OF_RANGE)
        );
    }

    #[test]
    fn rejects_non_positive_prf() {
        let caps = full_capabilities();
        let mut cfg = permissive_config();
        cfg.prf_hz = 0.0;
        assert_eq!(validate_config(&cfg, &caps), Err(error::PRF_RANGE_ILLEGAL));
    }

    #[test]
    fn rejects_range_extent_beyond_unambiguous_range() {
        let caps = full_capabilities();
        let mut cfg = permissive_config();
        // c/(2*1200 Hz) ≈ 125 km; pedir 500 celdas * 500 m = 250 km no cabe.
        cfg.prf_hz = 1200.0;
        cfg.gate_spacing_m = 500.0;
        assert_eq!(validate_config(&cfg, &caps), Err(error::PRF_RANGE_ILLEGAL));
    }

    #[test]
    fn accepts_range_extent_within_unambiguous_range() {
        let caps = full_capabilities();
        let mut cfg = permissive_config();
        cfg.prf_hz = 300.0;
        cfg.n_gates = 500;
        cfg.gate_spacing_m = 250.0; // 125 km, bajo el límite ~500 km
        assert_eq!(validate_config(&cfg, &caps), Ok(()));
    }
}
