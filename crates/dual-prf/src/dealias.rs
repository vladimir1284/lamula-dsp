//! Desdoblado dual-PRF por teorema chino del resto y corrección por
//! continuidad espacial (`tools/oracles/dual_prf_dealiasing.ipynb`).

/// Envuelve `v` al intervalo `[-v_a, v_a)` de periodo `2·v_a`.
pub fn fold(v: f64, v_a: f64) -> f64 {
    ((v + v_a).rem_euclid(2.0 * v_a)) - v_a
}

pub struct DualPrfEstimate {
    /// Velocidad desdoblada, el candidato que mejor reconcilia `v1_meas` y
    /// `v2_meas`.
    pub velocity_mps: f64,
    /// Error residual (m/s) tras plegar el candidato al intervalo de
    /// Nyquist de la PRF alta y compararlo con `v2_meas`.
    pub residual_mps: f64,
}

/// Desdobla la velocidad radial comparando las medidas pulse-pair de dos
/// PRFs en razón simple. Prueba cada múltiplo de plegado `n1` de la PRF baja
/// (`v1_meas + 2·v_a1·n1`) dentro de la Nyquist extendida y elige el que,
/// al plegarlo al intervalo de la PRF alta, mejor reconcilia con `v2_meas`.
pub fn dealias_dual_prf(
    v1_meas: f64,
    v2_meas: f64,
    v_a1: f64,
    v_a2: f64,
    v_ext: f64,
) -> DualPrfEstimate {
    assert!(
        v_a1 > 0.0 && v_a2 > 0.0 && v_ext > 0.0,
        "las velocidades de Nyquist y la extendida deben ser positivas"
    );

    let n_range = (v_ext / (2.0 * v_a1)).ceil() as i64 + 1;
    let mut best_err = f64::INFINITY;
    let mut best_v = v1_meas;

    for n1 in -n_range..=n_range {
        let v_cand = v1_meas + 2.0 * v_a1 * n1 as f64;
        if v_cand.abs() > v_ext + v_a1 {
            continue;
        }
        let err_raw = (fold(v_cand, v_a2) - v2_meas).abs();
        let err = err_raw.min(2.0 * v_a2 - err_raw);
        if err < best_err {
            best_err = err;
            best_v = v_cand;
        }
    }

    DualPrfEstimate {
        velocity_mps: best_v,
        residual_mps: best_err,
    }
}

/// Corrección por continuidad espacial: recorrido secuencial en el que cada
/// celda se ajusta al múltiplo de `2·v_a1` que más la acerca a la celda
/// vecina ya corregida. La página lo describe como estructural al método,
/// no un accesorio: en ciertos puntos de la Nyquist extendida dos múltiplos
/// de plegado reconcilian casi igual de bien incluso sin ruido.
pub fn continuity_fix(v_hats: &[f64], v_a1: f64, max_fold_search: i64) -> Vec<f64> {
    let mut fixed = v_hats.to_vec();
    for i in 1..v_hats.len() {
        let neighbor_ref = fixed[i - 1];
        let mut best_k = 0i64;
        let mut best_diff = (v_hats[i] - neighbor_ref).abs();
        for k in -max_fold_search..=max_fold_search {
            let cand = v_hats[i] + k as f64 * 2.0 * v_a1;
            let diff = (cand - neighbor_ref).abs();
            if diff < best_diff {
                best_diff = diff;
                best_k = k;
            }
        }
        fixed[i] = v_hats[i] + best_k as f64 * 2.0 * v_a1;
    }
    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_wraps_to_nyquist_interval() {
        assert!((fold(0.0, 10.0) - 0.0).abs() < 1e-9);
        assert!((fold(15.0, 10.0) - (-5.0)).abs() < 1e-9);
        assert!((fold(-15.0, 10.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn dealias_recovers_unfolded_velocity_without_noise() {
        const V_A1: f64 = 20.833333333333332; // λ/(4·1.2ms), λ=0.10
        const V_A2: f64 = 31.25; // λ/(4·0.8ms)
        const V_EXT: f64 = 3.0 * V_A1;

        let v_true = 25.0;
        let v1_meas = fold(v_true, V_A1);
        let v2_meas = fold(v_true, V_A2);

        let est = dealias_dual_prf(v1_meas, v2_meas, V_A1, V_A2, V_EXT);
        assert!(
            (est.velocity_mps - v_true).abs() < 1e-6,
            "v_hat={:.4}",
            est.velocity_mps
        );
    }

    #[test]
    fn continuity_fix_corrects_isolated_fold_jump() {
        const V_A1: f64 = 20.0;
        let v_true = 20.0;
        let mut v_hats = vec![v_true; 10];
        v_hats[5] += 2.0 * V_A1; // salto de un pliegue en una sola celda
        let fixed = continuity_fix(&v_hats, V_A1, 3);
        assert!((fixed[5] - v_true).abs() < 1e-9);
    }
}
