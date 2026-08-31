//! Mediana de una muestra, usada por los dos procedimientos del crate —
//! robusta frente a alguna celda contaminada, celdas "Birdbath" y "ΦDP de
//! sistema" del oráculo.

/// Mediana de `values`. Promedia los dos centrales en muestras de tamaño par.
pub(crate) fn median(values: &[f64]) -> f64 {
    assert!(!values.is_empty(), "la muestra no puede estar vacía");
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("valores NaN en la muestra"));
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_length_returns_middle() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
    }

    #[test]
    fn even_length_averages_middle_two() {
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.5);
    }

    #[test]
    fn single_value() {
        assert_eq!(median(&[5.0]), 5.0);
    }
}
