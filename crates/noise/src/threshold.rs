//! Resta de ruido y censura por `sig_threshold`.
//!
//! Ver `docs/algorithms/ruido-y-umbrales.md` §"Resta" y §"Umbrales":
//! `log_threshold`, `sqi_threshold` y `ccor_threshold` censuran por
//! cantidades que calculan otros algoritmos del pipeline (potencia
//! logarítmica absoluta, índices de calidad, corrección de clutter) y no
//! tienen oráculo en esta página — no se implementan aquí.

/// `S = max(R(0) - N, 0)`. La resta se hace en lineal; el recorte a cero
/// evita propagar potencia negativa (posible con `N` estimado sobre una
/// realización finita) a un logaritmo. `None` marca la celda sin señal
/// detectable — no "potencia negativa muy pequeña".
pub fn subtract_noise(r0: f64, noise_floor: f64) -> Option<f64> {
    let s = r0 - noise_floor;
    (s > 0.0).then_some(s)
}

/// `SNR = 10·log10(signal / noise_floor)`, dB.
pub fn snr_db(signal: f64, noise_floor: f64) -> f64 {
    assert!(noise_floor > 0.0, "noise_floor debe ser positivo");
    10.0 * (signal / noise_floor).log10()
}

/// `true` si la celda debe censurarse por `sig_threshold` (SNR insuficiente).
/// Una celda ya censurada por `subtract_noise` (sin señal detectable) se
/// censura igualmente por esta vía si se le asigna `snr_db = -inf`.
pub fn censored_by_sig_threshold(snr_db: f64, sig_threshold_db: f64) -> bool {
    snr_db <= sig_threshold_db
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtract_noise_clips_at_zero() {
        assert_eq!(subtract_noise(0.5, 1.0), None);
        assert_eq!(subtract_noise(1.0, 1.0), None);
        assert!(subtract_noise(1.5, 1.0).is_some());
    }

    #[test]
    fn censored_below_or_at_threshold() {
        assert!(censored_by_sig_threshold(3.0, 3.0));
        assert!(censored_by_sig_threshold(2.9, 3.0));
        assert!(!censored_by_sig_threshold(3.1, 3.0));
        assert!(censored_by_sig_threshold(f64::NEG_INFINITY, 3.0));
    }
}
