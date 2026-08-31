//! Promediado de celda gruesa (`docs/algorithms/procesamiento-de-rango.md`
//! §"Resolución y promediado en rango"): media aritmética de `K` estimaciones
//! independientes reduce su varianza por el factor `K`. La página fija que
//! ganar muestras independientes de verdad exige promediar muestras I/Q antes
//! de estimar, no potencias después — esta función es el paso final común a
//! los dos caminos una vez se tienen `K` valores a combinar.

/// Media aritmética de `K` valores de potencia (o de cualquier estimador
/// combinable linealmente) de celdas de rango contiguas.
pub fn average_power(powers: &[f64]) -> f64 {
    assert!(!powers.is_empty(), "hace falta al menos un valor");
    powers.iter().sum::<f64>() / powers.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_values() {
        assert!((average_power(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < 1e-12);
    }
}
