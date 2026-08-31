//! Ajuste de mínimos cuadrados en ventana deslizante (Ryzhkov & Zrnić 1996).
//!
//! `docs/algorithms/kdp-estimacion.md` §"Cómo funciona" y celda
//! `kdp_window_fit` del oráculo: pendiente por mínimos cuadrados de
//! ΦDP(r) ya desdoblado en una ventana de `window_gates` celdas centrada en
//! cada punto, dividida por dos.

/// Estima KDP (grados/km si `gate_spacing_km` está en km) por ventana
/// deslizante de mínimos cuadrados sobre `phidp_unwrapped_deg`. `None` en las
/// celdas donde la ventana recortada por el borde del perfil tiene menos de
/// tres puntos (recta indeterminada).
pub fn kdp_window_fit(
    phidp_unwrapped_deg: &[f64],
    gate_spacing_km: f64,
    window_gates: usize,
) -> Vec<Option<f64>> {
    let n = phidp_unwrapped_deg.len();
    let half = window_gates / 2;
    let r_km: Vec<f64> = (0..n).map(|i| i as f64 * gate_spacing_km).collect();

    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(n);
            if hi - lo < 3 {
                return None;
            }
            let x = &r_km[lo..hi];
            let y = &phidp_unwrapped_deg[lo..hi];
            let count = x.len() as f64;
            let x_bar = x.iter().sum::<f64>() / count;
            let y_bar = y.iter().sum::<f64>() / count;
            let mut num = 0.0;
            let mut den = 0.0;
            for (&xi, &yi) in x.iter().zip(y.iter()) {
                num += (xi - x_bar) * (yi - y_bar);
                den += (xi - x_bar) * (xi - x_bar);
            }
            if den == 0.0 {
                return None;
            }
            Some(num / den / 2.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_slope_profile_recovers_kdp() {
        // PhiDP(r) = 2*K0*r -> KDP = K0 exactamente, sin ruido.
        const K0: f64 = 2.0;
        const SPACING_KM: f64 = 0.150;
        let n = 50;
        let phidp: Vec<f64> = (0..n).map(|i| 2.0 * K0 * i as f64 * SPACING_KM).collect();
        let kdp = kdp_window_fit(&phidp, SPACING_KM, 15);
        for (i, k) in kdp.iter().enumerate() {
            let k = k.unwrap_or_else(|| panic!("celda {i} sin estimar"));
            assert!((k - K0).abs() < 1e-9, "celda {i}: kdp={k}");
        }
    }

    #[test]
    fn too_short_profile_is_none() {
        let phidp = vec![0.0, 1.0];
        let kdp = kdp_window_fit(&phidp, 0.150, 15);
        assert!(kdp.iter().all(|k| k.is_none()));
    }
}
