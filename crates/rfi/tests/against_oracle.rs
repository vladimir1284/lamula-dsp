//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/rfi_filtrado.ipynb`.
//! Reproduce sus tolerancias exactas con un número de realizaciones
//! recortado para mantener el test rápido. Verifica la composición
//! completa: detección de este crate + relleno gaussiano de
//! `lamula-clutter` (el mismo mecanismo, reutilizado sin reimplementar).

use lamula_clutter::{gmap_filter, moments_from_spectrum};
use lamula_noise::noise_floor_estimate;
use lamula_rfi::{detect_rfi_mask, DEFAULT_RFI_MEDIAN_DB, DEFAULT_RFI_WIDTH_MAX_BINS};
use lamula_simulator::gaussian_doppler_spectrum;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const M: usize = 256;
const K_AVERAGES: usize = 10;

fn complex_gaussian(rng: &mut impl Rng, variance: f64) -> Complex64 {
    let sigma = (variance / 2.0).sqrt();
    let re: f64 = StandardNormal.sample(rng);
    let im: f64 = StandardNormal.sample(rng);
    Complex64::new(re * sigma, im * sigma)
}

/// `generate_cell_full` del oráculo: meteoro + clutter estacionario
/// opcional en v=0 + tono de RFI opcional, incoherente con el eco (fase
/// inicial aleatoria).
#[allow(clippy::too_many_arguments)]
fn generate_cell_full(
    power_weather: f64,
    mean_v: f64,
    sigma_v: f64,
    power_clutter: f64,
    power_rfi: f64,
    rfi_bin: usize,
    noise_floor: f64,
    rng: &mut impl Rng,
) -> Vec<Complex64> {
    let weather = gaussian_doppler_spectrum(power_weather, mean_v, sigma_v, WAVELENGTH_M, PRT_S, M);
    let clutter = if power_clutter > 0.0 {
        gaussian_doppler_spectrum(power_clutter, 0.0, 1e-6, WAVELENGTH_M, PRT_S, M)
    } else {
        vec![0.0; M]
    };
    let shaped: Vec<Complex64> = weather
        .iter()
        .zip(&clutter)
        .map(|(&w, &c)| complex_gaussian(rng, 1.0) * (w + c).sqrt())
        .collect();
    let mut planner = FftPlanner::new();
    let ifft = planner.plan_fft_inverse(M);
    let mut y = shaped;
    ifft.process(&mut y);

    if power_rfi > 0.0 {
        let phase0 = rng.gen_range(-std::f64::consts::PI..std::f64::consts::PI);
        for (n, x) in y.iter_mut().enumerate() {
            let phase = 2.0 * std::f64::consts::PI * rfi_bin as f64 * n as f64 / M as f64 + phase0;
            *x += Complex64::from_polar(power_rfi.sqrt(), phase);
        }
    }
    if noise_floor > 0.0 {
        for x in y.iter_mut() {
            *x += complex_gaussian(rng, noise_floor);
        }
    }
    y
}

fn hann_window(m: usize) -> Vec<f64> {
    let denom = (m - 1) as f64;
    (0..m)
        .map(|n| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / denom).cos())
        .collect()
}

fn windowed_periodogram(y: &[Complex64], win: &[f64]) -> Vec<f64> {
    let m = y.len();
    let s2: f64 = win.iter().map(|w| w * w).sum();
    let mut buf: Vec<Complex64> = y.iter().zip(win).map(|(&s, &w)| s * w).collect();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(m);
    fft.process(&mut buf);
    let denom = m as f64 * s2;
    buf.iter().map(|c| c.norm_sqr() / denom).collect()
}

fn bin_velocity(k: usize, m: usize) -> f64 {
    let half = m.div_ceil(2);
    let k_signed = if k < half {
        k as i64
    } else {
        k as i64 - m as i64
    };
    let f_k = k_signed as f64 / (m as f64 * PRT_S);
    f_k * WAVELENGTH_M / 2.0
}

fn v_k_axis(m: usize) -> Vec<f64> {
    (0..m).map(|k| bin_velocity(k, m)).collect()
}

#[allow(clippy::too_many_arguments)]
fn averaged_periodogram(
    power_weather: f64,
    mean_v: f64,
    sigma_v: f64,
    power_clutter: f64,
    power_rfi: f64,
    rfi_bin: usize,
    noise_floor: f64,
    win: &[f64],
    rng: &mut impl Rng,
) -> (Vec<f64>, f64) {
    let mut acc = vec![0.0f64; M];
    let mut n_hat_sum = 0.0f64;
    for _ in 0..K_AVERAGES {
        let y = generate_cell_full(
            power_weather,
            mean_v,
            sigma_v,
            power_clutter,
            power_rfi,
            rfi_bin,
            noise_floor,
            rng,
        );
        n_hat_sum += noise_floor_estimate(&y);
        for (a, p) in acc.iter_mut().zip(windowed_periodogram(&y, win)) {
            *a += p;
        }
    }
    let k = K_AVERAGES as f64;
    for a in acc.iter_mut() {
        *a /= k;
    }
    (acc, n_hat_sum / k / M as f64)
}

/// Prueba 1 del oráculo: meteoro + RFI fuerte y lejos del meteoro. Con el
/// filtro activo, los momentos deben coincidir con el escenario sin RFI;
/// sin filtro, la RFI arruina la velocidad.
#[test]
fn recovers_weather_moments_with_strong_rfi_far_from_peak() {
    const POWER_WEATHER: f64 = 1.0;
    const MEAN_V: f64 = 5.0;
    const SIGMA_V: f64 = 1.5;
    const NOISE_FLOOR: f64 = 0.05;
    const RFI_BIN: usize = 200;
    const POWER_RFI: f64 = 20.0;
    const N_TRIALS: usize = 300;

    let win = hann_window(M);
    let v_k = v_k_axis(M);
    let v_a = WAVELENGTH_M / (4.0 * PRT_S);
    let bin_spacing = 2.0 * v_a / M as f64;
    let mut rng = StdRng::seed_from_u64(20260910);

    let mut z_clean_sum = 0.0;
    let mut v_clean_sum = 0.0;
    let mut n_clean = 0usize;
    let mut z_filtered_sum = 0.0;
    let mut v_filtered_sum = 0.0;
    let mut n_filtered = 0usize;
    let mut v_unfiltered_sum = 0.0;
    let mut n_unfiltered = 0usize;

    for _ in 0..N_TRIALS {
        let (p_clean, n_clean_thresh) = averaged_periodogram(
            POWER_WEATHER,
            MEAN_V,
            SIGMA_V,
            0.0,
            0.0,
            RFI_BIN,
            NOISE_FLOOR,
            &win,
            &mut rng,
        );
        let m_clean = moments_from_spectrum(&p_clean, &v_k, bin_spacing, n_clean_thresh);
        if let Some(v) = m_clean.velocity_mps {
            z_clean_sum += m_clean.power_linear;
            v_clean_sum += v;
            n_clean += 1;
        }

        let (p_rfi, n_rfi_thresh) = averaged_periodogram(
            POWER_WEATHER,
            MEAN_V,
            SIGMA_V,
            0.0,
            POWER_RFI,
            RFI_BIN,
            NOISE_FLOOR,
            &win,
            &mut rng,
        );
        let m_unfiltered = moments_from_spectrum(&p_rfi, &v_k, bin_spacing, n_rfi_thresh);
        if let Some(v) = m_unfiltered.velocity_mps {
            v_unfiltered_sum += v;
            n_unfiltered += 1;
        }

        let mask = detect_rfi_mask(&p_rfi, DEFAULT_RFI_MEDIAN_DB, DEFAULT_RFI_WIDTH_MAX_BINS);
        let fixed = gmap_filter(&p_rfi, &v_k, &mask, n_rfi_thresh, 3.0);
        let m_filtered = moments_from_spectrum(&fixed.filtered, &v_k, bin_spacing, n_rfi_thresh);
        if let Some(v) = m_filtered.velocity_mps {
            z_filtered_sum += m_filtered.power_linear;
            v_filtered_sum += v;
            n_filtered += 1;
        }
    }

    let z_clean_mean = z_clean_sum / n_clean as f64;
    let v_clean_mean = v_clean_sum / n_clean as f64;
    let z_filtered_mean = z_filtered_sum / n_filtered as f64;
    let v_filtered_mean = v_filtered_sum / n_filtered as f64;
    let v_unfiltered_mean = v_unfiltered_sum / n_unfiltered as f64;

    assert!(
        (z_filtered_mean - z_clean_mean).abs() < 0.2 * z_clean_mean,
        "Z con filtro RFI={z_filtered_mean:.4} se aleja de la verdad sin RFI={z_clean_mean:.4}"
    );
    assert!(
        (v_filtered_mean - v_clean_mean).abs() < 1.0,
        "V con filtro RFI={v_filtered_mean:.4} se aleja de la verdad sin RFI={v_clean_mean:.4}"
    );
    assert!(
        (v_unfiltered_mean - MEAN_V).abs() > 5.0,
        "sin filtro la RFI no arruina V lo suficiente para justificar el filtro: V_hat={v_unfiltered_mean:.4}"
    );
}

/// Prueba 2 del oráculo, la que más importa: sobre señal limpia (sin RFI)
/// con un meteoro fuerte, la tasa de falsos positivos debe ser baja y, si
/// dispara, no debe alterar los momentos frente a no filtrar.
#[test]
fn false_positive_rate_on_clean_signal_is_low() {
    const POWER_WEATHER: f64 = 1.0;
    const MEAN_V: f64 = 5.0;
    const SIGMA_V: f64 = 1.5;
    const NOISE_FLOOR: f64 = 0.05;
    const N_TRIALS: usize = 300;

    let win = hann_window(M);
    let v_k = v_k_axis(M);
    let v_a = WAVELENGTH_M / (4.0 * PRT_S);
    let bin_spacing = 2.0 * v_a / M as f64;
    let mut rng = StdRng::seed_from_u64(20260910);

    let mut false_positives = 0usize;
    let mut z_on_sum = 0.0;
    let mut z_off_sum = 0.0;
    let mut v_on_sum = 0.0;
    let mut v_off_sum = 0.0;
    let mut n_ok = 0usize;

    for _ in 0..N_TRIALS {
        let (p_clean, n_thresh) = averaged_periodogram(
            POWER_WEATHER,
            MEAN_V,
            SIGMA_V,
            0.0,
            0.0,
            0,
            NOISE_FLOOR,
            &win,
            &mut rng,
        );
        let mask = detect_rfi_mask(&p_clean, DEFAULT_RFI_MEDIAN_DB, DEFAULT_RFI_WIDTH_MAX_BINS);
        if mask.iter().any(|&m| m) {
            false_positives += 1;
        }
        let fixed = gmap_filter(&p_clean, &v_k, &mask, n_thresh, 3.0);
        let m_on = moments_from_spectrum(&fixed.filtered, &v_k, bin_spacing, n_thresh);
        let m_off = moments_from_spectrum(&p_clean, &v_k, bin_spacing, n_thresh);
        if let (Some(v_on), Some(v_off)) = (m_on.velocity_mps, m_off.velocity_mps) {
            z_on_sum += m_on.power_linear;
            z_off_sum += m_off.power_linear;
            v_on_sum += v_on;
            v_off_sum += v_off;
            n_ok += 1;
        }
    }

    let fp_rate = false_positives as f64 / N_TRIALS as f64;
    let z_diff = (z_on_sum / n_ok as f64 - z_off_sum / n_ok as f64).abs();
    let v_diff = (v_on_sum / n_ok as f64 - v_off_sum / n_ok as f64).abs();

    assert!(
        fp_rate < 0.05,
        "tasa de falsos positivos={fp_rate:.3} excede 5%"
    );
    assert!(
        z_diff < 0.02 * POWER_WEATHER,
        "diferencia de Z con/sin filtro sobre señal limpia={z_diff:.5} excede tolerancia"
    );
    assert!(
        v_diff < 0.2,
        "diferencia de V con/sin filtro sobre señal limpia={v_diff:.5} excede tolerancia"
    );
}

/// Prueba 3 del oráculo: filtrar RFI antes que clutter recupera Z; el
/// orden inverso deja que la RFI contamine el ajuste de GMAP y es peor.
#[test]
fn filtering_rfi_before_clutter_beats_the_reverse_order() {
    const POWER_WEATHER: f64 = 1.0;
    const SIGMA_V: f64 = 1.5;
    const NOISE_FLOOR: f64 = 0.05;
    const POWER_CLUTTER: f64 = 100.0;
    const POWER_RFI_SHOULDER: f64 = 15.0;
    const N_TRIALS: usize = 300;

    let win = hann_window(M);
    let v_k = v_k_axis(M);
    let v_a = WAVELENGTH_M / (4.0 * PRT_S);
    let bin_spacing = 2.0 * v_a / M as f64;
    let clutter_width_ms = 4.0 * bin_spacing;
    let clutter_mask: Vec<bool> = v_k
        .iter()
        .map(|&v| v.abs() <= clutter_width_ms / 2.0)
        .collect();

    // Bin más cercano a v=4 m/s: dentro del "hombro" que GMAP usaría para
    // ajustar el modelo gaussiano de la banda de clutter.
    let rfi_bin = (0..M)
        .min_by(|&a, &b| {
            (v_k[a] - 4.0)
                .abs()
                .partial_cmp(&(v_k[b] - 4.0).abs())
                .unwrap()
        })
        .unwrap();

    let mut rng = StdRng::seed_from_u64(20260910);
    let mut z_rfi_first_sum = 0.0;
    let mut z_clutter_first_sum = 0.0;
    let mut n_ok = 0usize;

    for _ in 0..N_TRIALS {
        let (p_raw, n_hat) = averaged_periodogram(
            POWER_WEATHER,
            0.0,
            SIGMA_V,
            POWER_CLUTTER,
            POWER_RFI_SHOULDER,
            rfi_bin,
            NOISE_FLOOR,
            &win,
            &mut rng,
        );

        let rfi_mask = detect_rfi_mask(&p_raw, DEFAULT_RFI_MEDIAN_DB, DEFAULT_RFI_WIDTH_MAX_BINS);
        let no_rfi = gmap_filter(&p_raw, &v_k, &rfi_mask, n_hat, 3.0);
        let correct = gmap_filter(&no_rfi.filtered, &v_k, &clutter_mask, n_hat, 3.0);
        let m_correct = moments_from_spectrum(&correct.filtered, &v_k, bin_spacing, n_hat);

        let no_clutter = gmap_filter(&p_raw, &v_k, &clutter_mask, n_hat, 3.0);
        let rfi_mask2 = detect_rfi_mask(
            &no_clutter.filtered,
            DEFAULT_RFI_MEDIAN_DB,
            DEFAULT_RFI_WIDTH_MAX_BINS,
        );
        let wrong = gmap_filter(&no_clutter.filtered, &v_k, &rfi_mask2, n_hat, 3.0);
        let m_wrong = moments_from_spectrum(&wrong.filtered, &v_k, bin_spacing, n_hat);

        if let (Some(_), Some(_)) = (m_correct.velocity_mps, m_wrong.velocity_mps) {
            z_rfi_first_sum += m_correct.power_linear;
            z_clutter_first_sum += m_wrong.power_linear;
            n_ok += 1;
        }
    }

    let z_correct_mean = z_rfi_first_sum / n_ok as f64;
    let z_wrong_mean = z_clutter_first_sum / n_ok as f64;

    assert!(
        (z_correct_mean - POWER_WEATHER).abs() < 0.3 * POWER_WEATHER,
        "orden correcto no recupera Z dentro del 30%: Z_hat={z_correct_mean:.4}"
    );
    assert!(
        (z_wrong_mean - POWER_WEATHER).abs() > (z_correct_mean - POWER_WEATHER).abs(),
        "el orden incorrecto no es medible como peor que el correcto (correcto={z_correct_mean:.4} incorrecto={z_wrong_mean:.4})"
    );
}
