//! Ventana de Hann y su ganancia de ruido equivalente (ENBW).

/// Ventana de Hann simétrica de `m` puntos, misma convención que
/// `numpy.hanning`: `w[n] = 0.5 - 0.5·cos(2πn/(m-1))`, `n = 0..m`. Para
/// `m == 1` devuelve `[1.0]` (numpy también lo hace, evitando la división
/// por cero de `m-1`).
pub fn hann_window(m: usize) -> Vec<f64> {
    if m <= 1 {
        return vec![1.0; m];
    }
    (0..m)
        .map(|n| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / (m - 1) as f64).cos())
        .collect()
}

/// Ancho de banda de ruido equivalente de la ventana, en bins (Harris 1978):
/// `ENBW = M·Σw² / (Σw)²`. Es el factor por el que un suelo de ruido blanco
/// aparece escalado en una traza normalizada por ganancia coherente.
pub fn enbw_bins(win: &[f64]) -> f64 {
    assert!(!win.is_empty(), "la ventana no puede estar vacía");
    let m = win.len() as f64;
    let sum: f64 = win.iter().sum();
    let sum_sq: f64 = win.iter().map(|w| w * w).sum();
    m * sum_sq / (sum * sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_window_matches_known_endpoints() {
        let w = hann_window(8);
        assert!((w[0] - 0.0).abs() < 1e-9);
        assert!((w[7] - 0.0).abs() < 1e-9);
        // Simétrica.
        for i in 0..w.len() {
            assert!((w[i] - w[w.len() - 1 - i]).abs() < 1e-9);
        }
    }

    #[test]
    fn enbw_of_rectangular_window_is_one() {
        let rect = vec![1.0; 32];
        assert!((enbw_bins(&rect) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn enbw_of_hann_is_about_one_point_five() {
        let win = hann_window(64);
        let enbw = enbw_bins(&win);
        assert!((enbw - 1.5).abs() < 0.05, "enbw={enbw}");
    }
}
