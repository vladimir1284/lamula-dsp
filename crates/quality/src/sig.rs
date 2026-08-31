//! SIG (Signal-to-Noise Ratio tras resta de ruido) —
//! `docs/algorithms/indices-de-calidad.md` §"SIG".

/// `SIG = 10·log10(S / N)`, dB, con `s_linear` ya recortado a la censura de
/// `docs/algorithms/ruido-y-umbrales.md` (`lamula_noise::subtract_noise`).
/// `None` cuando `s_linear <= 0`: celda sin señal detectable, SIG no
/// definido — misma censura que la resta de ruido, no una segunda regla.
pub fn sig_db(s_linear: f64, noise_floor: f64) -> Option<f64> {
    assert!(noise_floor > 0.0, "noise_floor debe ser positivo");
    (s_linear > 0.0).then(|| 10.0 * (s_linear / noise_floor).log10())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn censored_below_or_at_zero_signal() {
        assert_eq!(sig_db(0.0, 1.0), None);
        assert_eq!(sig_db(-0.5, 1.0), None);
    }

    #[test]
    fn equal_signal_and_noise_is_zero_db() {
        assert_eq!(sig_db(2.0, 2.0), Some(0.0));
    }
}
