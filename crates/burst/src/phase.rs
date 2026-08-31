//! Medida de fase y frecuencia del burst, y corrección de fase por
//! coherent-on-receive (`docs/algorithms/burst-fase-afc.md` §"Medida del
//! burst" y §"Corrección de fase").

use std::f64::consts::PI;

use rustfft::num_complex::Complex64;

/// Fase inicial del pulso: argumento de la suma coherente de las muestras del
/// burst. Promediar antes de tomar el argumento —no al revés— evita el salto
/// de fase en ±π.
pub fn burst_phase_estimate(burst: &[Complex64]) -> f64 {
    assert!(!burst.is_empty(), "el burst no puede estar vacío");
    let mean: Complex64 = burst.iter().sum::<Complex64>() / burst.len() as f64;
    mean.arg()
}

/// Frecuencia del burst: argumento de la autocovarianza a retardo 1 dentro
/// del burst, dividido por `2π·dt_fast_s` (`dt_fast_s` es el periodo de
/// muestreo *dentro* de la ventana de burst, no el PRT del rayo).
pub fn burst_freq_estimate(burst: &[Complex64], dt_fast_s: f64) -> f64 {
    assert!(burst.len() >= 2, "hacen falta al menos dos muestras");
    assert!(dt_fast_s > 0.0, "dt_fast_s debe ser positivo");

    let mut r1 = Complex64::new(0.0, 0.0);
    for w in burst.windows(2) {
        r1 += w[0] * w[1].conj();
    }
    r1 /= (burst.len() - 1) as f64;
    -r1.arg() / (2.0 * PI * dt_fast_s)
}

/// Corrige una muestra de eco del pulso `m` multiplicándola por
/// `exp(-j·phi_m)`, con `phi_m` la fase medida del burst de ese mismo pulso.
pub fn correct_phase(sample: Complex64, phi: f64) -> Complex64 {
    sample * Complex64::from_polar(1.0, -phi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::Rng;
    use rand::SeedableRng;
    use rand_distr::{Distribution, StandardNormal};

    fn complex_gaussian(rng: &mut StdRng, variance: f64) -> Complex64 {
        let sigma = (variance / 2.0).sqrt();
        let re: f64 = StandardNormal.sample(rng);
        let im: f64 = StandardNormal.sample(rng);
        Complex64::new(re * sigma, im * sigma)
    }

    const AMP_BURST: f64 = 10.0;
    const BURST_NOISE_VAR: f64 = 0.01; // SNR de burst ~ 40 dB
    const N_BURST: usize = 32;

    fn simulated_burst_phase(
        rng: &mut StdRng,
        phi_true: f64,
        amp: f64,
        noise_var: f64,
        n: usize,
    ) -> Vec<Complex64> {
        (0..n)
            .map(|_| Complex64::from_polar(amp, phi_true) + complex_gaussian(rng, noise_var))
            .collect()
    }

    fn simulated_burst_freq(
        rng: &mut StdRng,
        phi0: f64,
        f_true: f64,
        amp: f64,
        noise_var: f64,
        n: usize,
        dt_fast_s: f64,
    ) -> Vec<Complex64> {
        (0..n)
            .map(|i| {
                let phase = phi0 + 2.0 * PI * f_true * i as f64 * dt_fast_s;
                Complex64::from_polar(amp, phase) + complex_gaussian(rng, noise_var)
            })
            .collect()
    }

    /// Contraste con `tools/oracles/burst_fase_afc.ipynb` celda 4: sin sesgo
    /// (< 0.01 rad) en todo [-π,π], dispersión acotada (< 0.05 rad).
    #[test]
    fn phase_estimate_unbiased_across_full_circle() {
        const N_TRIALS: usize = 3000;
        let mut rng = StdRng::seed_from_u64(20260904);

        let errors: Vec<f64> = (0..N_TRIALS)
            .map(|_| {
                let phi_true = rng.gen_range(-PI..PI);
                let burst =
                    simulated_burst_phase(&mut rng, phi_true, AMP_BURST, BURST_NOISE_VAR, N_BURST);
                let phi_hat = burst_phase_estimate(&burst);
                (Complex64::from_polar(1.0, phi_hat - phi_true)).arg()
            })
            .collect();

        let mean = errors.iter().sum::<f64>() / N_TRIALS as f64;
        let std = (errors.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / N_TRIALS as f64).sqrt();

        assert!(
            mean.abs() < 0.01,
            "sesgo de fase {mean:.5} rad excede 0.01 rad"
        );
        assert!(
            std < 0.05,
            "dispersión de fase {std:.5} rad excede 0.05 rad"
        );
    }

    /// Celda 6 del oráculo: sesgo de frecuencia despreciable (< 200 Hz).
    #[test]
    fn freq_estimate_unbiased() {
        const N_TRIALS: usize = 3000;
        const DT_FAST_S: f64 = 100e-9;
        const F_OFFSET_HZ: f64 = 50_000.0;

        let mut rng = StdRng::seed_from_u64(20260904);
        let freq_mean: f64 = (0..N_TRIALS)
            .map(|_| {
                let phi0 = rng.gen_range(-PI..PI);
                let burst = simulated_burst_freq(
                    &mut rng,
                    phi0,
                    F_OFFSET_HZ,
                    AMP_BURST,
                    BURST_NOISE_VAR,
                    N_BURST,
                    DT_FAST_S,
                );
                burst_freq_estimate(&burst, DT_FAST_S)
            })
            .sum::<f64>()
            / N_TRIALS as f64;

        let bias_hz = freq_mean - F_OFFSET_HZ;
        assert!(
            bias_hz.abs() < 200.0,
            "sesgo de frecuencia {bias_hz:.1} Hz excede 200 Hz"
        );
    }
}
