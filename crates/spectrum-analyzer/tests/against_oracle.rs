//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra
//! `tools/oracles/analizador_espectro_fi.ipynb`. Reproduce sus tolerancias
//! exactas con un número de realizaciones recortado en la prueba de
//! varianza (la del oráculo es 1500 por punto de malla) para mantener el
//! test rápido.

use lamula_spectrum_analyzer::{enbw_bins, hann_window, welch_trace_dbm};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, StandardNormal, Uniform};
use rustfft::num_complex::Complex64;

const M: usize = 64;
const REF_LEVEL_OFFSET_DBM: f64 = -30.0;
const K_AVERAGES: usize = 20;
const TONE_POWER_LIN: f64 = 2.0;
const NOISE_FLOOR_TONE_TEST: f64 = 0.001;
const LEVEL_TOLERANCE_DB: f64 = 0.5;
const NOISE_VAR: f64 = 0.5;

fn complex_gaussian(rng: &mut impl Rng, variance: f64) -> Complex64 {
    let sigma = (variance / 2.0).sqrt();
    let re: f64 = StandardNormal.sample(rng);
    let im: f64 = StandardNormal.sample(rng);
    Complex64::new(re * sigma, im * sigma)
}

fn generate_tone_capture(
    bin_index: usize,
    power_lin: f64,
    m: usize,
    noise_floor: f64,
    rng: &mut impl Rng,
) -> Vec<Complex64> {
    let phase0: f64 = Uniform::new(-std::f64::consts::PI, std::f64::consts::PI).sample(rng);
    let mut y: Vec<Complex64> = (0..m)
        .map(|n| {
            Complex64::from_polar(
                power_lin.sqrt(),
                2.0 * std::f64::consts::PI * bin_index as f64 * n as f64 / m as f64 + phase0,
            )
        })
        .collect();
    if noise_floor > 0.0 {
        for x in y.iter_mut() {
            *x += complex_gaussian(rng, noise_floor);
        }
    }
    y
}

fn generate_noise_capture(noise_var: f64, m: usize, rng: &mut impl Rng) -> Vec<Complex64> {
    (0..m).map(|_| complex_gaussian(rng, noise_var)).collect()
}

/// Prueba 1 — posición y nivel del pico, incluido el borde.
#[test]
fn tone_peak_position_and_level_across_span() {
    let mut rng = StdRng::seed_from_u64(20260922);
    let win = hann_window(M);
    let expected_dbm = 10.0 * TONE_POWER_LIN.log10() + REF_LEVEL_OFFSET_DBM;

    for &bin_idx in &[0usize, 1, 32, 63] {
        let captures: Vec<Vec<Complex64>> = (0..K_AVERAGES)
            .map(|_| {
                generate_tone_capture(bin_idx, TONE_POWER_LIN, M, NOISE_FLOOR_TONE_TEST, &mut rng)
            })
            .collect();
        let trace = welch_trace_dbm(&captures, &win, REF_LEVEL_OFFSET_DBM);
        let (peak_bin, &peak_val) = trace
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        assert_eq!(peak_bin, bin_idx, "peak_bin={peak_bin} esperado={bin_idx}");
        assert!(
            (peak_val - expected_dbm).abs() < LEVEL_TOLERANCE_DB,
            "bin={bin_idx} peak_val={peak_val} expected={expected_dbm}"
        );
    }
}

/// Prueba 2 — nivel medio sobre ruido blanco: la traza reproduce
/// `ruido_var·ENBW`, no `ruido_var` sin corregir.
#[test]
fn noise_floor_level_matches_enbw_corrected_density() {
    let mut rng = StdRng::seed_from_u64(20260923);
    let win = hann_window(M);
    let enbw = enbw_bins(&win);

    let k_averages_noise = 500;
    let captures: Vec<Vec<Complex64>> = (0..k_averages_noise)
        .map(|_| generate_noise_capture(NOISE_VAR, M, &mut rng))
        .collect();
    // Traza en lineal (no dBm) para sumar potencia total sin pasar por log.
    let sum_w: f64 = win.iter().sum();
    let sw2 = sum_w * sum_w;
    let mut planner = rustfft::FftPlanner::new();
    let fft = planner.plan_fft_forward(M);
    let mut avg_power = vec![0.0f64; M];
    for capture in &captures {
        let mut buf: Vec<Complex64> = capture
            .iter()
            .zip(win.iter())
            .map(|(&x, &w)| x * w)
            .collect();
        fft.process(&mut buf);
        for (acc, x) in avg_power.iter_mut().zip(buf.iter()) {
            *acc += x.norm_sqr() / sw2;
        }
    }
    let n = captures.len() as f64;
    let measured_total: f64 = avg_power.iter().map(|p| p / n).sum();

    let expected_uncorrected = NOISE_VAR;
    let expected_corrected = NOISE_VAR * enbw;

    assert!(
        (measured_total - expected_uncorrected).abs() > 0.2 * expected_uncorrected,
        "measured_total={measured_total}"
    );
    assert!(
        (measured_total - expected_corrected).abs() < 0.05 * expected_corrected,
        "measured_total={measured_total} expected_corrected={expected_corrected}"
    );
}

/// Prueba 3 — la varianza de la traza baja con el número de promedios en el
/// factor K esperado.
#[test]
fn trace_variance_decreases_with_averages() {
    let mut rng = StdRng::seed_from_u64(20260924);
    let win = hann_window(M);
    let test_bin = 20;
    let n_trials = 1200; // recortado de 1500 del oráculo -- necesita quedarse
                         // cerca de esa cifra: con menos, la varianza de la
                         // propia varianza estimada excede la tolerancia del
                         // 15% en K grandes de vez en cuando
    let k_grid = [1usize, 4, 16, 64];

    let sum_w: f64 = win.iter().sum();
    let sw2 = sum_w * sum_w;
    let mut planner = rustfft::FftPlanner::new();
    let fft = planner.plan_fft_forward(M);

    let power_at_test_bin = |captures: &[Vec<Complex64>]| -> f64 {
        let mut sum = 0.0;
        for capture in captures {
            let mut buf: Vec<Complex64> = capture
                .iter()
                .zip(win.iter())
                .map(|(&x, &w)| x * w)
                .collect();
            fft.process(&mut buf);
            sum += buf[test_bin].norm_sqr() / sw2;
        }
        sum / captures.len() as f64
    };

    let mut variances = std::collections::HashMap::new();
    for &k in &k_grid {
        let samples: Vec<f64> = (0..n_trials)
            .map(|_| {
                let captures: Vec<Vec<Complex64>> = (0..k)
                    .map(|_| generate_noise_capture(NOISE_VAR, M, &mut rng))
                    .collect();
                power_at_test_bin(&captures)
            })
            .collect();
        let mean = samples.iter().sum::<f64>() / n_trials as f64;
        let var = samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (n_trials - 1) as f64;
        variances.insert(k, var);
    }

    let var1 = variances[&1];
    for &k in &k_grid[1..] {
        let ratio = var1 / variances[&k];
        assert!(
            (ratio - k as f64).abs() < 0.15 * k as f64,
            "K={k} ratio={ratio}"
        );
    }
}
