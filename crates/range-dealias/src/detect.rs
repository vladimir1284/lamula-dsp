//! Detección y marcado de trip múltiple por comparación dual-PRF.
//!
//! `docs/algorithms/dealiasing-de-rango.md` §"Cómo funciona" y celda
//! "Prueba 1" del oráculo: un eco de primer trip aparece en la misma
//! posición con las dos PRFs; uno de trip superior se desplaza, porque
//! `r_max` es distinto en cada barrido. Se prueban las dos hipótesis para la
//! posición leída en el barrido de PRF alta (la que sufre solapamiento) y se
//! elige la que reconcilia mejor con la posición leída en el barrido de PRF
//! baja.

/// Resultado de la reconciliación dual-PRF para una celda del barrido de PRF
/// alta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripClassification {
    /// El rango verdadero es la posición aparente en el barrido de PRF alta.
    Trip1,
    /// El rango verdadero es la posición aparente más `r_max` del barrido de
    /// PRF alta: la celda debe marcarse (`ray_flag.censored`) o corregirse
    /// si hay recuperación activa.
    Trip2,
}

/// Clasifica una celda del barrido de PRF alta comparando su posición
/// aparente con la del mismo azimut en el barrido de PRF baja.
/// `apparent_low_prf_m` es `None` cuando esa referencia no está disponible
/// (p.ej. el modo de corte no incluye un barrido de PRF baja coincidente):
/// en ese caso se asume `Trip1`, la hipótesis conservadora — es lo que
/// `docs/algorithms/dealiasing-de-rango.md` describe como "poco, pero
/// honesto": sin referencia no hay base para acusar solapamiento.
pub fn classify_trip(
    apparent_high_prf_m: f64,
    apparent_low_prf_m: Option<f64>,
    r_max_high_prf_m: f64,
) -> TripClassification {
    assert!(r_max_high_prf_m > 0.0, "r_max_high_prf_m debe ser positivo");

    let Some(apparent_low) = apparent_low_prf_m else {
        return TripClassification::Trip1;
    };

    let err_trip1 = (apparent_high_prf_m - apparent_low).abs();
    let err_trip2 = (apparent_high_prf_m + r_max_high_prf_m - apparent_low).abs();

    if err_trip2 < err_trip1 {
        TripClassification::Trip2
    } else {
        TripClassification::Trip1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_positions_are_trip1() {
        let result = classify_trip(30_000.0, Some(30_000.0), 90_000.0);
        assert_eq!(result, TripClassification::Trip1);
    }

    #[test]
    fn folded_position_is_trip2() {
        // Verdadero a 120 km, PRF alta lo pliega a 30 km (r_max=90 km); PRF
        // baja lo ve correctamente a 120 km.
        let result = classify_trip(30_000.0, Some(120_000.0), 90_000.0);
        assert_eq!(result, TripClassification::Trip2);
    }

    #[test]
    fn missing_low_prf_reference_defaults_to_trip1() {
        let result = classify_trip(30_000.0, None, 90_000.0);
        assert_eq!(result, TripClassification::Trip1);
    }
}
