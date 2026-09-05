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

/// Convierte una corrección de frecuencia (offset respecto de la nominal,
/// Hz — típicamente [`AfcLoop::freq_hz`]) a la palabra de fase absoluta del
/// NCO que transporta el mensaje `Afc` del contrato `DRx↔DSP`
/// (`contract/vendor/drx_dsp_v0_1.rs`, campo `nco_phase_inc`;
/// `docs/algorithms/burst-fase-afc.md` §"Lazo de AFC"; decisión D-02 del
/// proyecto DRx). Convención DDS estándar de acumulador de fase:
/// `phase_inc = round(freq_offset_hz / fs_hz * 2^word_bits)`, envuelto módulo
/// `2^word_bits` — un offset negativo sale como el complemento correspondiente
/// dentro de esa anchura, tal como espera un acumulador que envuelve módulo
/// `2^word_bits` sin signo.
///
/// **`fs_hz` y `word_bits` no vienen de ningún contrato de este repositorio.**
/// Ni `DRx↔DSP` ni `DSP↔RCP` exponen la frecuencia de referencia ni la
/// anchura del acumulador de fase del NCO de recepción del DRx —
/// `docs/dsp-plan.md` sólo documenta 250 MSPS como reloj del ADC, y nada en
/// este repositorio confirma si el NCO usa ese mismo reloj o uno derivado.
/// Mismo tipo de hueco sin campo propio en el contrato que
/// `MAGNETRON_TRANSMITTER`/`ZPHI_A_COEF_DB_PER_DEG` en `crates/service::ray`
/// (ver `docs/algorithms/roadmap.md` §"Decisiones cerradas"): quien llame
/// tiene que pasar los valores reales del hardware DRx, confirmados contra su
/// especificación, antes de comisionar contra hardware real — esta función
/// sólo hace la aritmética, no fija esos dos valores.
pub fn nco_phase_inc_for_freq_offset(freq_offset_hz: f64, fs_hz: f64, word_bits: u32) -> u64 {
    assert!(fs_hz > 0.0, "fs_hz debe ser positivo");
    assert!(
        (1..=63).contains(&word_bits),
        "word_bits debe estar en 1..=63 (evita desbordar el acumulador u64)"
    );
    let modulus = (1u64 << word_bits) as f64;
    let raw = (freq_offset_hz / fs_hz) * modulus;
    raw.round().rem_euclid(modulus) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nco_phase_inc_is_zero_for_zero_offset() {
        assert_eq!(nco_phase_inc_for_freq_offset(0.0, 250e6, 32), 0);
    }

    #[test]
    fn nco_phase_inc_matches_quarter_turn() {
        // offset = fs/4, 8 bits: un cuarto de vuelta = 256/4 = 64.
        let word = nco_phase_inc_for_freq_offset(25.0, 100.0, 8);
        assert_eq!(word, 64);
    }

    #[test]
    fn nco_phase_inc_wraps_negative_offset_to_twos_complement_equivalent() {
        // offset = -fs/4, 8 bits: envuelve a 256 - 64 = 192.
        let word = nco_phase_inc_for_freq_offset(-25.0, 100.0, 8);
        assert_eq!(word, 192);
    }

    #[test]
    fn nco_phase_inc_wraps_at_full_scale() {
        // offset = fs (una vuelta completa) envuelve a 0, no a `modulus`.
        let word = nco_phase_inc_for_freq_offset(100.0, 100.0, 8);
        assert_eq!(word, 0);
    }

    #[test]
    #[should_panic(expected = "fs_hz")]
    fn nco_phase_inc_rejects_non_positive_fs() {
        nco_phase_inc_for_freq_offset(1.0, 0.0, 8);
    }

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
