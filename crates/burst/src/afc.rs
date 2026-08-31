//! Lazo de control automático de frecuencia
//! (`docs/algorithms/burst-fase-afc.md` §"Lazo de AFC"): filtro de primer
//! orden sobre la frecuencia medida en el burst de cada rayo, con
//! congelamiento y BITE ante pérdida de burst.

use rustfft::num_complex::Complex64;

use crate::phase::burst_freq_estimate;

/// Ganancia de un lazo de primer orden con constante de tiempo `tau_s`
/// muestreado cada `update_period_s`: `1 - exp(-update_period_s/tau_s)`.
pub fn loop_gain(update_period_s: f64, tau_s: f64) -> f64 {
    assert!(update_period_s > 0.0, "update_period_s debe ser positivo");
    assert!(tau_s > 0.0, "tau_s debe ser positivo");
    1.0 - (-update_period_s / tau_s).exp()
}

/// Resultado de una actualización del lazo de AFC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AfcUpdate {
    /// Estimación de frecuencia filtrada tras esta actualización, Hz.
    pub freq_hz: f64,
    /// `true` si esta actualización se congeló por pérdida de burst — la
    /// amplitud medida cayó por debajo de `amp_threshold`.
    pub bite: bool,
}

/// Lazo de AFC de primer orden. Se alimenta un burst por rayo con
/// [`AfcLoop::update`]; el estado (`freq_hz`) persiste entre llamadas.
pub struct AfcLoop {
    gain: f64,
    amp_threshold: f64,
    dt_fast_s: f64,
    freq_hz: f64,
}

impl AfcLoop {
    /// `gain` es la ganancia del lazo (ver [`loop_gain`]), `amp_threshold` el
    /// umbral de amplitud de burst por debajo del cual se declara pérdida, y
    /// `dt_fast_s` el periodo de muestreo dentro de la ventana de burst. El
    /// estado arranca en `freq_hz = 0.0`.
    pub fn new(gain: f64, amp_threshold: f64, dt_fast_s: f64) -> Self {
        assert!((0.0..=1.0).contains(&gain), "gain debe estar en [0,1]");
        Self {
            gain,
            amp_threshold,
            dt_fast_s,
            freq_hz: 0.0,
        }
    }

    /// Frecuencia estimada vigente (última actualización válida).
    pub fn freq_hz(&self) -> f64 {
        self.freq_hz
    }

    /// Procesa un burst: si su amplitud media cae por debajo de
    /// `amp_threshold`, el lazo se congela en su último valor válido y
    /// `bite` se marca. En caso contrario mide la frecuencia del burst y
    /// avanza el filtro de primer orden `freq_hz += gain·(f_meas - freq_hz)`.
    pub fn update(&mut self, burst: &[Complex64]) -> AfcUpdate {
        let mean: Complex64 = burst.iter().sum::<Complex64>() / burst.len() as f64;
        let amp_meas = mean.norm();

        if amp_meas < self.amp_threshold {
            return AfcUpdate {
                freq_hz: self.freq_hz,
                bite: true,
            };
        }

        let f_meas = burst_freq_estimate(burst, self.dt_fast_s);
        self.freq_hz += self.gain * (f_meas - self.freq_hz);
        AfcUpdate {
            freq_hz: self.freq_hz,
            bite: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_matches_first_order_formula() {
        let g = loop_gain(0.02, 2.0);
        assert!((g - (1.0 - (-0.01f64).exp())).abs() < 1e-12);
    }

    #[test]
    fn freezes_and_flags_bite_on_low_amplitude_burst() {
        let mut loop_ = AfcLoop::new(0.5, 2.0, 100e-9);
        let strong = vec![Complex64::new(5.0, 0.0); 32];
        let weak = vec![Complex64::new(0.1, 0.0); 32];

        let before = loop_.update(&strong);
        assert!(!before.bite);

        let during = loop_.update(&weak);
        assert!(during.bite);
        assert_eq!(during.freq_hz, before.freq_hz);
    }
}
