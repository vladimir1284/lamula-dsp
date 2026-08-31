//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/indices_de_calidad.ipynb`.
//! Reproduce sus tolerancias exactas con un número de realizaciones recortado
//! para mantener el test rápido.

use std::f64::consts::PI;

use lamula_noise::noise_floor_estimate;
use lamula_quality::{ccor_db, sig_db, sqi};
use lamula_simulator::{gaussian_doppler_spectrum, generate_cell, CellParams};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const MEAN_V: f64 = 5.0;

fn complex_gaussian(rng: &mut impl Rng, variance: f64) -> Complex64 {
    let sigma = (variance / 2.0).sqrt();
    let re: f64 = StandardNormal.sample(rng);
    let im: f64 = StandardNormal.sample(rng);
    Complex64::new(re * sigma, im * sigma)
}

fn r0_r1(y: &[Complex64]) -> (f64, Complex64) {
    let r0 = y.iter().map(|s| s.norm_sqr()).sum::<f64>() / y.len() as f64;
    let mut r1 = Complex64::new(0.0, 0.0);
    for w in y.windows(2) {
        r1 += w[0] * w[1].conj();
    }
    r1 /= (y.len() - 1) as f64;
    (r0, r1)
}

/// Blanco puntual/tono puro exacto: toda la potencia en un único bin Doppler
/// discreto, no una gaussiana de σv diminuto (eso subdesborda a 0/0 antes de
/// llegar a ser "un tono") -- celda `generate_pure_tone_cell` del oráculo.
fn generate_pure_tone_cell(
    power_s: f64,
    bin_index: usize,
    m: usize,
    rng: &mut impl Rng,
) -> Vec<Complex64> {
    let mut spectral: Vec<Complex64> = (0..m).map(|_| complex_gaussian(rng, 1.0)).collect();
    for (k, s) in spectral.iter_mut().enumerate() {
        *s *= if k == bin_index { power_s.sqrt() } else { 0.0 };
    }
    let mut planner = FftPlanner::new();
    let ifft = planner.plan_fft_inverse(m);
    ifft.process(&mut spectral);
    spectral
}

/// Celda "SQI": el SQI esperado a SNR finita es `ρ(T)·SNR/(SNR+1)`, con
/// `ρ(T) = exp(-8π²σv²T²/λ²)` el modelo ACF gaussiano cerrado.
#[test]
fn sqi_follows_closed_form_across_grid() {
    const SQI_ABS_TOLERANCE: f64 = 0.08;
    const M: usize = 64;
    const NOISE_FLOOR: f64 = 0.05;
    const N_TRIALS: usize = 600;

    let mut rng = StdRng::seed_from_u64(20260906);
    let params = |power_s: f64, sigma_v: f64| CellParams {
        power_s,
        mean_v: MEAN_V,
        sigma_v,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: M,
        noise_floor: NOISE_FLOOR,
    };

    for &snr_db in &[0.0, 10.0, 20.0, 40.0] {
        let power_s = NOISE_FLOOR * 10f64.powf(snr_db / 10.0);
        let snr_lin = power_s / NOISE_FLOOR;
        for &sigma_v in &[0.5f64, 1.5, 3.0] {
            let rho_t =
                (-8.0 * PI.powi(2) * sigma_v.powi(2) * PRT_S.powi(2) / WAVELENGTH_M.powi(2)).exp();
            let expected = rho_t * snr_lin / (snr_lin + 1.0);

            let p = params(power_s, sigma_v);
            let measured: f64 = (0..N_TRIALS)
                .map(|_| {
                    let y = generate_cell(&p, &mut rng);
                    let (r0, r1) = r0_r1(&y);
                    sqi(r0, r1.norm())
                })
                .sum::<f64>()
                / N_TRIALS as f64;

            assert!(
                (measured - expected).abs() < SQI_ABS_TOLERANCE,
                "SNR={snr_db} sv={sigma_v}: SQI medido={measured:.4} esperado={expected:.4}"
            );
        }
    }
}

/// Extremos de SQI: ruido puro converge a la predicción de Rayleigh
/// `√π / (2√(M-1))`; tono puro da exactamente 1.0.
#[test]
fn sqi_extremes_match_closed_form() {
    const M: usize = 64;
    let mut rng = StdRng::seed_from_u64(20260906);

    const N_NOISE: usize = 2000;
    let noise_params = CellParams {
        power_s: 0.0,
        mean_v: 0.0,
        sigma_v: 1.0,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: M,
        noise_floor: 1.0,
    };
    let pure_noise_mean: f64 = (0..N_NOISE)
        .map(|_| {
            let y = generate_cell(&noise_params, &mut rng);
            let (r0, r1) = r0_r1(&y);
            sqi(r0, r1.norm())
        })
        .sum::<f64>()
        / N_NOISE as f64;
    let expected_noise_sqi = PI.sqrt() / (2.0 * ((M - 1) as f64).sqrt());
    assert!(
        (pure_noise_mean - expected_noise_sqi).abs() < 0.15 * expected_noise_sqi,
        "SQI ruido puro={pure_noise_mean:.4} esperado(Rayleigh)={expected_noise_sqi:.4}"
    );

    const N_TONE: usize = 200;
    let max_dev = (0..N_TONE)
        .map(|_| {
            let y = generate_pure_tone_cell(1.0, 10, M, &mut rng);
            let (r0, r1) = r0_r1(&y);
            (sqi(r0, r1.norm()) - 1.0).abs()
        })
        .fold(0.0f64, f64::max);
    assert!(
        max_dev < 1e-9,
        "SQI tono puro se desvía de 1.0 en {max_dev:e}"
    );
}

/// Celda "CCOR": inyecta clutter de potencia exactamente conocida en el bin
/// de velocidad cero y aplica un notch ideal, sólo para validar la
/// contabilidad de potencias y la conversión a dB -- no valida GMAP (fase 2,
/// página propia, sin implementación todavía).
#[test]
fn ccor_reproduces_known_power_ratio() {
    const POWER_S: f64 = 1.0;
    const CLUTTER_POWER: f64 = 50.0;
    const SIGMA_V_CCOR: f64 = 1.5;
    const M: usize = 64;
    const N_TRIALS: usize = 4000;
    const CCOR_TOLERANCE_DB: f64 = 0.5;

    let mut rng = StdRng::seed_from_u64(20260906);
    let mut planner = FftPlanner::new();
    let ifft = planner.plan_fft_inverse(M);
    let fft = planner.plan_fft_forward(M);

    let mut spectrum_clutter =
        gaussian_doppler_spectrum(POWER_S, MEAN_V, SIGMA_V_CCOR, WAVELENGTH_M, PRT_S, M);
    spectrum_clutter[0] += CLUTTER_POWER;

    let (mut total_sum, mut filtered_sum) = (0.0, 0.0);
    for _ in 0..N_TRIALS {
        let mut shaped: Vec<Complex64> = spectrum_clutter
            .iter()
            .map(|&s| complex_gaussian(&mut rng, 1.0) * s.sqrt())
            .collect();
        ifft.process(&mut shaped);
        let p_total = shaped.iter().map(|s| s.norm_sqr()).sum::<f64>() / M as f64;

        let mut spec = shaped.clone();
        fft.process(&mut spec);
        spec[0] = Complex64::new(0.0, 0.0);
        let mut filtered = spec;
        ifft.process(&mut filtered);
        // `ifft.process` es la IFFT sin normalizar de rustfft (igual que la
        // FFT hacia adelante); el oráculo aquí usa `np.fft.ifft` normalizada
        // (divide por M), así que hace falta un factor 1/M² extra en potencia
        // -- a diferencia de `p_total`, que replica la convención *M del
        // simulador (ver comentario de `shape_to_time_domain`).
        let p_filtered = filtered.iter().map(|s| s.norm_sqr()).sum::<f64>() / (M as f64).powi(3);

        total_sum += p_total;
        filtered_sum += p_filtered;
    }
    let p_total_mean = total_sum / N_TRIALS as f64;
    let p_filtered_mean = filtered_sum / N_TRIALS as f64;

    let spectrum_clean =
        gaussian_doppler_spectrum(POWER_S, MEAN_V, SIGMA_V_CCOR, WAVELENGTH_M, PRT_S, M);
    let bin0_power = spectrum_clean[0] + CLUTTER_POWER;
    let total_power_expected = POWER_S + CLUTTER_POWER;
    let filtered_power_expected = total_power_expected - bin0_power;
    let ccor_expected = 10.0 * (filtered_power_expected / total_power_expected).log10();

    let ccor_measured = ccor_db(p_total_mean, p_filtered_mean);
    assert!(
        (ccor_measured - ccor_expected).abs() < CCOR_TOLERANCE_DB,
        "CCOR medido={ccor_measured:.3} dB esperado={ccor_expected:.3} dB"
    );

    assert_eq!(ccor_db(p_total_mean, p_total_mean), 0.0);
}

/// Celda "SIG": SIG sigue la SNR inyectada dentro de 2 dB para SNR>=5 dB
/// (tolerancia amplia porque N̂ aparece en numerador y denominador, y el
/// sesgo conocido de HS74 se compone en vez de cancelarse).
#[test]
fn sig_follows_injected_snr() {
    const NOISE_FLOOR: f64 = 0.05;
    const N_TRIALS: usize = 2000;
    const SIG_BIAS_TOLERANCE_DB: f64 = 2.0;
    const M: usize = 64;

    let mut rng = StdRng::seed_from_u64(20260906);
    for &snr_db in &[5.0, 10.0, 20.0] {
        let power_s = NOISE_FLOOR * 10f64.powf(snr_db / 10.0);
        let p = CellParams {
            power_s,
            mean_v: MEAN_V,
            sigma_v: 1.5,
            wavelength_m: WAVELENGTH_M,
            prt_s: PRT_S,
            m: M,
            noise_floor: NOISE_FLOOR,
        };

        let mut estimates = Vec::new();
        for _ in 0..N_TRIALS {
            let y = generate_cell(&p, &mut rng);
            let (r0, _) = r0_r1(&y);
            let n_hat = noise_floor_estimate(&y);
            let s_hat = (r0 - n_hat).max(0.0);
            if let Some(sig) = sig_db(s_hat, n_hat) {
                estimates.push(sig);
            }
        }
        let sig_mean = estimates.iter().sum::<f64>() / estimates.len() as f64;
        assert!(
            (sig_mean - snr_db).abs() < SIG_BIAS_TOLERANCE_DB,
            "SNR={snr_db}: SIG medio={sig_mean:.3} dB excede tolerancia {SIG_BIAS_TOLERANCE_DB} dB"
        );
    }
}
