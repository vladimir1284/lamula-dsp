//! Conversión de cuenta cruda de encoder SSI (`azimuth_raw`/`elevation_raw`
//! de `RawPulseFrame`/[`crate::AssembledRadial`]) a grados.
//!
//! `docs/algorithms/procesamiento-de-rango.md` §"Ensamblado del radial": "hay
//! que convertirlos a grados con la resolución del encoder y su offset de
//! cero, ambos configuración" — ninguno de los dos vive en el contrato
//! `DRx↔DSP` (`contract/vendor/drx_dsp_v0_1.rs` sólo trae la cuenta cruda, sin
//! campo de resolución ni de cero) ni en `dsp_rcp::Config`
//! (`contract/schema/dsp_rcp_v0_1.toml`). Por eso [`ssi_counts_to_deg`] los
//! recibe como parámetros en vez de asumir un encoder de N bits: inventar esa
//! cifra sería inventar hardware. De dónde sale ese `counts_per_turn`/
//! `zero_offset_deg` en marcha (fichero de configuración del sitio, mensaje
//! nuevo del contrato, lo que sea) sigue sin dueño.
//!
//! No valida que `raw_counts < counts_per_turn`: una lectura fuera de rango
//! es un fallo de encoder que el DRx ya señaliza (`contract/vendor/drx_dsp_v0_1.rs`:
//! "Timeouts y tramas malas de los encoders", "Lectura de encoder inválida en
//! este rayo"), no algo que este cálculo deba re-detectar.

/// Cuenta cruda `raw_counts` de un encoder SSI de `counts_per_turn` cuentas
/// por vuelta, con offset de cero `zero_offset_deg`, a grados en `[0, 360)`.
///
/// # Panics
/// Si `counts_per_turn == 0` (invariante del llamador: un encoder no tiene
/// cero cuentas por vuelta).
pub fn ssi_counts_to_deg(raw_counts: u32, counts_per_turn: u32, zero_offset_deg: f64) -> f64 {
    assert!(counts_per_turn > 0, "counts_per_turn debe ser > 0");
    let raw_deg = (raw_counts as f64) * 360.0 / (counts_per_turn as f64);
    (raw_deg + zero_offset_deg).rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_counts_with_no_offset_is_zero_degrees() {
        assert_eq!(ssi_counts_to_deg(0, 4096, 0.0), 0.0);
    }

    #[test]
    fn quarter_turn_is_ninety_degrees() {
        assert_eq!(ssi_counts_to_deg(1024, 4096, 0.0), 90.0);
    }

    #[test]
    fn full_turn_wraps_to_zero() {
        assert_eq!(ssi_counts_to_deg(4096, 4096, 0.0), 0.0);
    }

    #[test]
    fn positive_offset_shifts_forward() {
        assert_eq!(ssi_counts_to_deg(0, 4096, 45.0), 45.0);
    }

    #[test]
    fn offset_larger_than_360_wraps() {
        assert_eq!(ssi_counts_to_deg(0, 4096, 365.0), 5.0);
    }

    #[test]
    fn negative_offset_wraps_into_positive_range() {
        let deg = ssi_counts_to_deg(0, 4096, -10.0);
        assert!((deg - 350.0).abs() < 1e-9);
    }

    #[test]
    #[should_panic(expected = "counts_per_turn debe ser > 0")]
    fn zero_counts_per_turn_panics() {
        ssi_counts_to_deg(0, 0, 0.0);
    }
}
