//! Desdoblado de un perfil de ángulos en grados, equivalente a `np.unwrap`.
//!
//! `docs/algorithms/kdp-estimacion.md` §"Cómo funciona": ΦDP se mide módulo
//! 360° y sólo puede crecer con el rango, así que un salto negativo grande
//! entre celdas contiguas es un pliegue, no una caída real.

/// Desdobla `profile_deg` acumulando correcciones de ±360° cuando el salto
/// entre celdas contiguas excede 180° en valor absoluto.
pub fn unwrap_deg(profile_deg: &[f64]) -> Vec<f64> {
    if profile_deg.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(profile_deg.len());
    out.push(profile_deg[0]);
    let mut correction = 0.0;
    for w in profile_deg.windows(2) {
        let mut delta = w[1] - w[0];
        while delta > 180.0 {
            delta -= 360.0;
            correction -= 360.0;
        }
        while delta < -180.0 {
            delta += 360.0;
            correction += 360.0;
        }
        out.push(w[1] + correction);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_wrap_leaves_profile_unchanged() {
        let profile = vec![0.0, 10.0, 25.0, 40.0];
        let unwrapped = unwrap_deg(&profile);
        for (a, b) in profile.iter().zip(unwrapped.iter()) {
            assert!((a - b).abs() < 1e-9);
        }
    }

    #[test]
    fn single_fold_is_corrected() {
        // 170 -> -170 es un salto de -340 (ó +20 real): se desdobla a 190.
        let profile = vec![170.0, -170.0, -160.0];
        let unwrapped = unwrap_deg(&profile);
        assert!((unwrapped[0] - 170.0).abs() < 1e-9);
        assert!((unwrapped[1] - 190.0).abs() < 1e-9);
        assert!((unwrapped[2] - 200.0).abs() < 1e-9);
    }

    #[test]
    fn empty_profile_returns_empty() {
        assert!(unwrap_deg(&[]).is_empty());
    }
}
