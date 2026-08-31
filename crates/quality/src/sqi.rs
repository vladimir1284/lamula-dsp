//! SQI (Signal Quality Index) — `docs/algorithms/indices-de-calidad.md` §"SQI".

/// `SQI = |R̂(1)| / R̂(0)`, calculado sobre la serie que se usó para estimar
/// la velocidad (sin resta de ruido — `r0_raw` es la potencia total antes de
/// restar el suelo de ruido, no `s_linear`). `r1_abs` es `|R̂(1)|`, el módulo
/// de la autocovarianza a retardo 1 que el pulse-pair ya calcula.
pub fn sqi(r0_raw: f64, r1_abs: f64) -> f64 {
    assert!(r0_raw > 0.0, "r0_raw debe ser positivo");
    assert!(r1_abs >= 0.0, "r1_abs no puede ser negativo");
    r1_abs / r0_raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_tone_is_exactly_one() {
        assert_eq!(sqi(1.0, 1.0), 1.0);
    }

    #[test]
    fn incoherent_ratio_is_zero() {
        assert_eq!(sqi(1.0, 0.0), 0.0);
    }
}
