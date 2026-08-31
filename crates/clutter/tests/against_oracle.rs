//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/gmap_clutter_filtering.ipynb`
//! y `tools/oracles/mapas_de_clutter.ipynb`. Reproduce sus tolerancias
//! exactas con un número de realizaciones recortado para mantener el test
//! rápido.

use lamula_clutter::{gmap_filter, is_clutter_cell, moments_from_spectrum, notch_filter};
use lamula_noise::noise_floor_estimate;
use lamula_simulator::gaussian_doppler_spectrum;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const M: usize = 64;
const K_AVERAGES: usize = 10;

fn complex_gaussian(rng: &mut impl Rng, variance: f64) -> Complex64 {
    let sigma = (variance / 2.0).sqrt();
    let re: f64 = StandardNormal.sample(rng);
    let im: f64 = StandardNormal.sample(rng);
    Complex64::new(re * sigma, im * sigma)
}

/// Celda meteoro + clutter -- `generate_cell_with_clutter` del oráculo de
/// GMAP: clutter modelado como blanco casi puntual (`sigma_v=1e-6`) en
/// v=0, sin dispersión de velocidad propia.
fn generate_cell_with_clutter(
    power_weather: f64,
    mean_v: f64,
    sigma_v: f64,
    power_clutter: f64,
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

fn clutter_mask(v_k: &[f64], half_width: f64) -> Vec<bool> {
    v_k.iter().map(|&v| v.abs() <= half_width).collect()
}

/// `averaged_periodogram` del oráculo: promedia `K_AVERAGES` periodogramas
/// ventaneados y umbrales de ruido independientes -- un solo barrido es
/// demasiado ruidoso bin a bin para que el ajuste de mínimos cuadrados de
/// GMAP signifique algo.
#[allow(clippy::too_many_arguments)]
fn averaged_periodogram(
    power_weather: f64,
    mean_v: f64,
    sigma_v: f64,
    power_clutter: f64,
    noise_floor: f64,
    win: &[f64],
    rng: &mut impl Rng,
) -> (Vec<f64>, f64) {
    let mut acc = vec![0.0f64; M];
    let mut n_hat_sum = 0.0f64;
    for _ in 0..K_AVERAGES {
        let y = generate_cell_with_clutter(
            power_weather,
            mean_v,
            sigma_v,
            power_clutter,
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

/// Prueba 1 del oráculo: meteoro con velocidad verdadera cero, exactamente
/// bajo un clutter 20 dB más fuerte -- el notch lo destruye, GMAP lo
/// recupera.
#[test]
fn gmap_recovers_weather_under_strong_clutter_at_v_zero() {
    const POWER_WEATHER: f64 = 1.0;
    const SIGMA_V: f64 = 1.5;
    const POWER_CLUTTER: f64 = 100.0;
    const NOISE_FLOOR: f64 = 0.05;
    const N_TRIALS: usize = 400;

    let v_a = WAVELENGTH_M / (4.0 * PRT_S);
    let bin_spacing = 2.0 * v_a / M as f64;
    let v_k = v_k_axis(M);
    // Ancho de banda = 4 espaciados de bin, igual que CLUTTER_WIDTH_MS del oráculo.
    let clutter_width_ms = 4.0 * bin_spacing;
    let mask = clutter_mask(&v_k, clutter_width_ms / 2.0);

    let win = hann_window(M);
    let mut rng = StdRng::seed_from_u64(20260908);

    let mut z_notch_sum = 0.0;
    let mut z_gmap_sum = 0.0;
    let mut n_notch = 0usize;
    let mut n_gmap = 0usize;

    for _ in 0..N_TRIALS {
        let (p_avg, n_thresh) = averaged_periodogram(
            POWER_WEATHER,
            0.0,
            SIGMA_V,
            POWER_CLUTTER,
            NOISE_FLOOR,
            &win,
            &mut rng,
        );

        let p_notch = notch_filter(&p_avg, &mask);
        let m_notch = moments_from_spectrum(&p_notch, &v_k, bin_spacing, n_thresh);
        if m_notch.velocity_mps.is_some() {
            z_notch_sum += m_notch.power_linear;
            n_notch += 1;
        }

        let gmap = gmap_filter(&p_avg, &v_k, &mask, n_thresh, 3.0);
        let m_gmap = moments_from_spectrum(&gmap.filtered, &v_k, bin_spacing, n_thresh);
        if m_gmap.velocity_mps.is_some() {
            z_gmap_sum += m_gmap.power_linear;
            n_gmap += 1;
        }
    }

    let z_notch_mean = z_notch_sum / n_notch as f64;
    let z_gmap_mean = z_gmap_sum / n_gmap as f64;

    assert!(
        (z_gmap_mean - POWER_WEATHER).abs() < 0.25 * POWER_WEATHER,
        "GMAP no recupera Z dentro del 25%: Z_hat={z_gmap_mean:.4} verdad={POWER_WEATHER}"
    );
    assert!(
        (z_notch_mean - POWER_WEATHER).abs() > 0.50 * POWER_WEATHER,
        "el notch no pierde suficiente Z para justificar el contraste: Z_hat={z_notch_mean:.4}"
    );
    assert!(
        (z_gmap_mean - POWER_WEATHER).abs() < 0.5 * (z_notch_mean - POWER_WEATHER).abs(),
        "GMAP no mejora al notch al menos 2x: gmap={z_gmap_mean:.4} notch={z_notch_mean:.4}"
    );
}

/// Prueba 2 del oráculo: sin clutter, filtro activo -- degradación mínima
/// de Z/V/W frente al espectro sin filtrar, en dos casos (meteoro en la
/// banda y lejos de ella).
#[test]
fn gmap_degradation_without_clutter_is_bounded() {
    const POWER_WEATHER: f64 = 1.0;
    const SIGMA_V: f64 = 1.5;
    const NOISE_FLOOR: f64 = 0.05;
    const N_TRIALS: usize = 400;
    const DEGRADATION_TOLERANCE_Z_FRAC: f64 = 0.15;
    const DEGRADATION_TOLERANCE_VW: f64 = 0.6;

    let v_a = WAVELENGTH_M / (4.0 * PRT_S);
    let bin_spacing = 2.0 * v_a / M as f64;
    let v_k = v_k_axis(M);
    let clutter_width_ms = 4.0 * bin_spacing;
    let mask = clutter_mask(&v_k, clutter_width_ms / 2.0);
    let win = hann_window(M);

    let mut rng = StdRng::seed_from_u64(20260908);
    for &mean_v_case in &[0.0, 8.0] {
        let mut raw = (0.0, 0.0, 0.0, 0usize);
        let mut gmap_acc = (0.0, 0.0, 0.0, 0usize);
        for _ in 0..N_TRIALS {
            let (p_avg, n_thresh) = averaged_periodogram(
                POWER_WEATHER,
                mean_v_case,
                SIGMA_V,
                0.0,
                NOISE_FLOOR,
                &win,
                &mut rng,
            );

            let m_raw = moments_from_spectrum(&p_avg, &v_k, bin_spacing, n_thresh);
            if let (Some(v), Some(w)) = (m_raw.velocity_mps, m_raw.spectrum_width_mps) {
                raw.0 += m_raw.power_linear;
                raw.1 += v;
                raw.2 += w;
                raw.3 += 1;
            }

            let gmap = gmap_filter(&p_avg, &v_k, &mask, n_thresh, 3.0);
            let m_gmap = moments_from_spectrum(&gmap.filtered, &v_k, bin_spacing, n_thresh);
            if let (Some(v), Some(w)) = (m_gmap.velocity_mps, m_gmap.spectrum_width_mps) {
                gmap_acc.0 += m_gmap.power_linear;
                gmap_acc.1 += v;
                gmap_acc.2 += w;
                gmap_acc.3 += 1;
            }
        }
        let (z_raw, v_raw, w_raw, n_raw) = raw;
        let (z_g, v_g, w_g, n_g) = gmap_acc;
        let (z_raw_m, v_raw_m, w_raw_m) = (
            z_raw / n_raw as f64,
            v_raw / n_raw as f64,
            w_raw / n_raw as f64,
        );
        let (z_g_m, v_g_m, w_g_m) = (z_g / n_g as f64, v_g / n_g as f64, w_g / n_g as f64);

        assert!(
            (z_g_m - z_raw_m).abs() < DEGRADATION_TOLERANCE_Z_FRAC * POWER_WEATHER,
            "mean_v={mean_v_case}: degradación de Z={:.4} excede tolerancia",
            (z_g_m - z_raw_m).abs()
        );
        assert!(
            (v_g_m - v_raw_m).abs() < DEGRADATION_TOLERANCE_VW
                && (w_g_m - w_raw_m).abs() < DEGRADATION_TOLERANCE_VW,
            "mean_v={mean_v_case}: degradación de V/W excede tolerancia (dV={:.4} dW={:.4})",
            (v_g_m - v_raw_m).abs(),
            (w_g_m - w_raw_m).abs()
        );
    }
}

/// Prueba 3 del oráculo: curva frente a razón clutter/señal (CSR) de 0 a
/// 40 dB, con el meteoro lejos de la banda de clutter -- Z sigue la
/// verdad-terreno en toda la curva.
#[test]
fn gmap_follows_truth_across_csr_curve() {
    const POWER_WEATHER: f64 = 1.0;
    const SIGMA_V: f64 = 1.5;
    const NOISE_FLOOR: f64 = 0.05;
    const N_TRIALS: usize = 250;
    const CURVE_TOLERANCE_Z_FRAC: f64 = 0.25;

    let v_a = WAVELENGTH_M / (4.0 * PRT_S);
    let bin_spacing = 2.0 * v_a / M as f64;
    let v_k = v_k_axis(M);
    let clutter_width_ms = 4.0 * bin_spacing;
    let mask = clutter_mask(&v_k, clutter_width_ms / 2.0);
    let win = hann_window(M);

    let mut rng = StdRng::seed_from_u64(20260908);
    for &csr_db in &[0.0, 10.0, 20.0, 30.0, 40.0] {
        let power_clutter = POWER_WEATHER * 10f64.powf(csr_db / 10.0);
        let mut z_sum = 0.0;
        let mut n_ok = 0usize;
        for _ in 0..N_TRIALS {
            let (p_avg, n_thresh) = averaged_periodogram(
                POWER_WEATHER,
                8.0,
                SIGMA_V,
                power_clutter,
                NOISE_FLOOR,
                &win,
                &mut rng,
            );
            let gmap = gmap_filter(&p_avg, &v_k, &mask, n_thresh, 3.0);
            let m_gmap = moments_from_spectrum(&gmap.filtered, &v_k, bin_spacing, n_thresh);
            if m_gmap.velocity_mps.is_some() {
                z_sum += m_gmap.power_linear;
                n_ok += 1;
            }
        }
        let z_mean = z_sum / n_ok as f64;
        assert!(
            (z_mean - POWER_WEATHER).abs() < CURVE_TOLERANCE_Z_FRAC * POWER_WEATHER,
            "CSR={csr_db}: Z_hat={z_mean:.4} fuera de tolerancia frente a verdad {POWER_WEATHER}"
        );
    }
}

/// Contraste del oráculo de mapas de clutter: tasas de detección y falsa
/// alarma del clasificador de persistencia sobre cuatro tipos de celda.
#[test]
fn clutter_map_classification_rates_match_oracle() {
    const POWER_CLUTTER: f64 = 5.0;
    const POWER_WEATHER: f64 = 5.0;
    const SIGMA_V_WEATHER: f64 = 2.0;
    const NOISE_FLOOR: f64 = 0.05;
    const N_SWEEPS: usize = 30;
    const AP_PRESENCE_FRAC: f64 = 0.4;
    const CV_THRESHOLD: f64 = 0.15;
    const POWER_THRESHOLD: f64 = 0.05;
    const N_CELLS: usize = 300;
    const DETECTION_MIN: f64 = 0.90;
    const FALSE_ALARM_MAX: f64 = 0.05;

    /// `power_estimate` del oráculo: potencia total menos ruido HS74,
    /// recortada a cero -- no la potencia cruda, que no distingue "clear"
    /// de una celda con señal débil.
    fn power_estimate(y: &[Complex64]) -> f64 {
        let r0 = y.iter().map(|s| s.norm_sqr()).sum::<f64>() / y.len() as f64;
        let n_hat = noise_floor_estimate(y);
        (r0 - n_hat).max(0.0)
    }

    fn static_clutter_cell(amp: Complex64, noise_floor: f64, rng: &mut impl Rng) -> Vec<Complex64> {
        (0..M)
            .map(|_| amp + complex_gaussian(rng, noise_floor))
            .collect()
    }

    fn weather_cell(
        power_s: f64,
        mean_v: f64,
        sigma_v: f64,
        noise_floor: f64,
        rng: &mut impl Rng,
    ) -> Vec<Complex64> {
        let spectrum = gaussian_doppler_spectrum(power_s, mean_v, sigma_v, WAVELENGTH_M, PRT_S, M);
        let shaped: Vec<Complex64> = spectrum
            .iter()
            .map(|&s| complex_gaussian(rng, 1.0) * s.sqrt())
            .collect();
        let mut planner = FftPlanner::new();
        let ifft = planner.plan_fft_inverse(M);
        let mut y = shaped;
        ifft.process(&mut y);
        if noise_floor > 0.0 {
            for x in y.iter_mut() {
                *x += complex_gaussian(rng, noise_floor);
            }
        }
        y
    }

    let mut rng = StdRng::seed_from_u64(20260909);
    for cell_type in ["clutter", "weather", "clear", "ap"] {
        let mut flagged = 0usize;
        for _ in 0..N_CELLS {
            let powers: Vec<f64> = match cell_type {
                "clutter" => {
                    let amp = complex_gaussian(&mut rng, POWER_CLUTTER);
                    (0..N_SWEEPS)
                        .map(|_| power_estimate(&static_clutter_cell(amp, NOISE_FLOOR, &mut rng)))
                        .collect()
                }
                "weather" => (0..N_SWEEPS)
                    .map(|_| {
                        power_estimate(&weather_cell(
                            POWER_WEATHER,
                            0.0,
                            SIGMA_V_WEATHER,
                            NOISE_FLOOR,
                            &mut rng,
                        ))
                    })
                    .collect(),
                "clear" => (0..N_SWEEPS)
                    .map(|_| power_estimate(&weather_cell(0.0, 0.0, 1.0, NOISE_FLOOR, &mut rng)))
                    .collect(),
                "ap" => {
                    let amp = complex_gaussian(&mut rng, POWER_CLUTTER);
                    (0..N_SWEEPS)
                        .map(|_| {
                            if rng.gen::<f64>() < AP_PRESENCE_FRAC {
                                power_estimate(&static_clutter_cell(amp, NOISE_FLOOR, &mut rng))
                            } else {
                                power_estimate(&weather_cell(0.0, 0.0, 1.0, NOISE_FLOOR, &mut rng))
                            }
                        })
                        .collect()
                }
                _ => unreachable!(),
            };
            if is_clutter_cell(&powers, POWER_THRESHOLD, CV_THRESHOLD) {
                flagged += 1;
            }
        }
        let rate = flagged as f64 / N_CELLS as f64;
        match cell_type {
            "clutter" => assert!(
                rate >= DETECTION_MIN,
                "tasa de detección de clutter={rate:.3} por debajo de {DETECTION_MIN}"
            ),
            _ => assert!(
                rate <= FALSE_ALARM_MAX,
                "{cell_type}: falsa alarma={rate:.3} por encima de {FALSE_ALARM_MAX}"
            ),
        }
    }
}
