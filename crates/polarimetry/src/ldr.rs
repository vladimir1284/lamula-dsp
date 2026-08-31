//! LDR (razón de despolarización lineal), sólo en modo alternante.
//!
//! `docs/algorithms/polarimetria-covarianzas.md` §"Cómo funciona" y celda
//! "LDR: saturación en el aislamiento de antena" del oráculo: razón de
//! potencias `10·log10(P_vh/P_hh)`, con resta de ruido por canal. La fuga de
//! aislamiento de antena no se modela aquí — es una propiedad del hardware
//! que ya está en la señal medida — sólo se declara el nivel para marcar el
//! resultado como no fiable por debajo de él.

use rustfft::num_complex::Complex64;

use lamula_noise::{noise_floor_estimate, total_power};

/// Margen sobre el aislamiento de antena bajo el cual LDR deja de ser
/// fiable: la página da el ejemplo concreto de 30 dB de aislamiento con
/// −27 dB aproximadamente como umbral, no −30 dB exacto — la fuga se
/// aproxima al suelo de aislamiento de forma asintótica y un umbral sin
/// margen queda pegado al ruido de la propia fuga
/// (`docs/algorithms/polarimetria-covarianzas.md` §"Cómo funciona").
const RELIABILITY_MARGIN_DB: f64 = 3.0;

/// Salida de la estimación de LDR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LdrEstimate {
    /// `10·log10(P_vh/P_hh)`, dB.
    pub ldr_db: f64,
    /// `false` cuando `ldr_db` está a menos de [`RELIABILITY_MARGIN_DB`] del
    /// aislamiento de antena configurado o por debajo de él: en ese rango el
    /// valor mide la fuga de la antena, no el meteoro.
    pub reliable: bool,
}

/// Potencia con resta de ruido, recortada a un mínimo positivo para que el
/// logaritmo nunca vea cero o negativo (celda "LDR" del oráculo,
/// `max(..., 1e-12)`).
fn clamped_power(y: &[Complex64]) -> f64 {
    let r0 = total_power(y);
    let n_hat = noise_floor_estimate(y);
    (r0 - n_hat).max(1e-12)
}

/// Estima LDR a partir de la copolar `hh` y la cruzada `vh` de una celda de
/// rango en modo alternante. `antenna_isolation_db` es el aislamiento de
/// polarización cruzada de la antena, positivo en dB (30.0 en el oráculo).
pub fn ldr_db(hh: &[Complex64], vh: &[Complex64], antenna_isolation_db: f64) -> LdrEstimate {
    assert!(
        antenna_isolation_db > 0.0,
        "antenna_isolation_db debe ser positivo"
    );

    let p_hh = clamped_power(hh);
    let p_vh = clamped_power(vh);
    let ldr_db = 10.0 * (p_vh / p_hh).log10();

    LdrEstimate {
        ldr_db,
        reliable: ldr_db > -antenna_isolation_db + RELIABILITY_MARGIN_DB,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_cross_pol_is_reliable() {
        let hh: Vec<Complex64> = (0..32).map(|_| Complex64::new(1.0, 0.0)).collect();
        let vh: Vec<Complex64> = (0..32).map(|_| Complex64::new(0.1, 0.0)).collect();
        let est = ldr_db(&hh, &vh, 30.0);
        assert!(est.reliable);
        assert!((est.ldr_db - (-20.0)).abs() < 1e-6);
    }

    #[test]
    fn cross_pol_below_isolation_is_unreliable() {
        let hh: Vec<Complex64> = (0..32).map(|_| Complex64::new(1.0, 0.0)).collect();
        let vh: Vec<Complex64> = (0..32).map(|_| Complex64::new(1.0e-4, 0.0)).collect();
        let est = ldr_db(&hh, &vh, 30.0);
        assert!(!est.reliable);
    }
}
