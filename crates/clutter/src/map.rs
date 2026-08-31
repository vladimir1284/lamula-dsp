//! Clasificador de persistencia para la generación del mapa de clutter
//! (`docs/algorithms/mapas-de-clutter.md`): distingue clutter de tierra
//! —misma amplitud barrido tras barrido, salvo ruido— de cualquier otro eco
//! persistente por su coeficiente de variación temporal.

pub struct ClutterCellStats {
    /// Media de la potencia estimada a lo largo de los barridos.
    pub mean_power: f64,
    /// Coeficiente de variación temporal, `std/media`; `+inf` si la media es
    /// cero o negativa.
    pub cv: f64,
}

/// Media y coeficiente de variación (desviación estándar poblacional sobre
/// media) de una serie de potencias estimadas, una por barrido.
pub fn clutter_cell_stats(powers: &[f64]) -> ClutterCellStats {
    assert!(!powers.is_empty(), "hace falta al menos un barrido");
    let n = powers.len() as f64;
    let mean_power = powers.iter().sum::<f64>() / n;
    let variance = powers
        .iter()
        .map(|&p| (p - mean_power).powi(2))
        .sum::<f64>()
        / n;
    let std = variance.sqrt();
    let cv = if mean_power > 0.0 {
        std / mean_power
    } else {
        f64::INFINITY
    };
    ClutterCellStats { mean_power, cv }
}

/// Marca una celda como clutter si su potencia media supera
/// `power_threshold` y su variación temporal está por debajo de
/// `cv_threshold` — persistencia de amplitud, no sólo de potencia media, para
/// no confundir clutter con meteoro estancado ni con propagación anómala
/// intermitente.
pub fn is_clutter_cell(powers: &[f64], power_threshold: f64, cv_threshold: f64) -> bool {
    let stats = clutter_cell_stats(powers);
    stats.mean_power > power_threshold && stats.cv < cv_threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_power_has_zero_cv() {
        let powers = vec![5.0; 30];
        let stats = clutter_cell_stats(&powers);
        assert_eq!(stats.mean_power, 5.0);
        assert_eq!(stats.cv, 0.0);
        assert!(is_clutter_cell(&powers, 0.05, 0.15));
    }

    #[test]
    fn zero_mean_power_has_infinite_cv() {
        let powers = vec![0.0; 10];
        let stats = clutter_cell_stats(&powers);
        assert!(stats.cv.is_infinite());
        assert!(!is_clutter_cell(&powers, 0.05, 0.15));
    }
}
