//! CCOR (Clutter Correction) — `docs/algorithms/indices-de-calidad.md` §"CCOR".

/// `CCOR = 10·log10(P_filtrada / P_total)`, dB. Negativo o cero por
/// construcción: `p_filtered` nunca excede `p_total` porque el filtro sólo
/// quita potencia. Cuando el filtro de clutter está inactivo, el llamador
/// pasa `p_filtered == p_total` y el resultado es exactamente `0.0`.
pub fn ccor_db(p_total: f64, p_filtered: f64) -> f64 {
    assert!(p_total > 0.0, "p_total debe ser positivo");
    assert!(p_filtered >= 0.0, "p_filtered no puede ser negativo");
    assert!(
        p_filtered <= p_total,
        "p_filtered no puede exceder p_total: el filtro sólo quita potencia"
    );
    10.0 * (p_filtered / p_total).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_filter_is_zero_db() {
        assert_eq!(ccor_db(3.7, 3.7), 0.0);
    }

    #[test]
    fn full_removal_is_negative_infinity() {
        assert_eq!(ccor_db(1.0, 0.0), f64::NEG_INFINITY);
    }
}
