//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra
//! `tools/oracles/staggered_prt_clutter_sz2000.ipynb`. Reproduce sus
//! tolerancias exactas con un número de realizaciones recortado para
//! mantener el test rápido. Generador de I/Q propio del oráculo (Cholesky
//! de `C[i,j] = R(t_i - t_j)` en los tiempos de muestreo reales, no
//! uniformes), con una segunda componente de la misma forma funcional para
//! el clutter (velocidad media cero, ancho espectral casi nulo).

#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_arguments)]

use lamula_staggered_prt::{reflectivity_estimate, staggered_pulse_pair_velocities};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;

const WAVELENGTH_M: f64 = 0.10;
const T1: f64 = 0.8e-3;
const T2: f64 = 1.2e-3; // razón 2:3
const TS: f64 = T1 + T2;
const M_PULSES: usize = 64; // subsecuencias de 32
const HALF_WIDTH_BINS: usize = 2;
const NOISE_FLOOR: f64 = 0.05;
const POWER_WEATHER: f64 = 1.0;
const SIGMA_V_TEST: f64 = 1.5;
const V_SAFE_MPS: f64 = 8.0; // lejos de la banda de notch de cada subsecuencia (>10 bins)

fn v_a_sub() -> f64 {
    WAVELENGTH_M / (4.0 * TS)
}

fn analytic_acf(power_s: f64, mean_v: f64, sigma_v: f64, tau: f64) -> Complex64 {
    let phase = Complex64::from_polar(
        1.0,
        4.0 * std::f64::consts::PI * mean_v * tau / WAVELENGTH_M,
    );
    let decay = (-8.0 * std::f64::consts::PI.powi(2) * sigma_v.powi(2) * tau.powi(2)
        / WAVELENGTH_M.powi(2))
    .exp();
    phase * (power_s * decay)
}

fn staggered_times(m: usize) -> Vec<f64> {
    let mut times = vec![0.0f64; m];
    for i in 1..m {
        let dt = if (i - 1) % 2 == 0 { T1 } else { T2 };
        times[i] = times[i - 1] + dt;
    }
    times
}

fn cholesky(c: &[Vec<Complex64>], m: usize) -> Vec<Vec<Complex64>> {
    let mut l = vec![vec![Complex64::new(0.0, 0.0); m]; m];
    for i in 0..m {
        for j in 0..=i {
            let mut sum = c[i][j];
            for k in 0..j {
                sum -= l[i][k] * l[j][k].conj();
            }
            if i == j {
                l[i][j] = Complex64::new(sum.re.max(0.0).sqrt(), 0.0);
            } else {
                l[i][j] = sum / l[j][j];
            }
        }
    }
    l
}

/// Genera una ráfaga escalonada con meteoro + clutter (opcional) + ruido,
/// misma construcción que la celda del oráculo.
fn generate_staggered_cell_with_clutter(
    mean_v: f64,
    sigma_v: f64,
    power_clutter: f64,
    m: usize,
    rng: &mut StdRng,
) -> Vec<Complex64> {
    const CLUTTER_SIGMA_V: f64 = 1.0e-3;

    let times = staggered_times(m);
    let mut c = vec![vec![Complex64::new(0.0, 0.0); m]; m];
    for i in 0..m {
        for j in 0..m {
            let tau = times[i] - times[j];
            let mut val = analytic_acf(POWER_WEATHER, mean_v, sigma_v, tau);
            if power_clutter > 0.0 {
                val += analytic_acf(power_clutter, 0.0, CLUTTER_SIGMA_V, tau);
            }
            c[i][j] = val;
        }
    }
    for i in 0..m {
        for j in 0..m {
            let avg = (c[i][j] + c[j][i].conj()) / 2.0;
            c[i][j] = avg;
        }
        c[i][i] += Complex64::new(1e-9 * (POWER_WEATHER + power_clutter), 0.0);
    }

    let l = cholesky(&c, m);
    let z: Vec<Complex64> = (0..m)
        .map(|_| {
            let re: f64 = StandardNormal.sample(rng);
            let im: f64 = StandardNormal.sample(rng);
            Complex64::new(re, im) / std::f64::consts::SQRT_2
        })
        .collect();

    let mut x = vec![Complex64::new(0.0, 0.0); m];
    for i in 0..m {
        let mut sum = Complex64::new(0.0, 0.0);
        for k in 0..=i {
            sum += l[i][k] * z[k];
        }
        x[i] = sum;
    }

    let sigma_n = (NOISE_FLOOR / 2.0).sqrt();
    for xi in x.iter_mut() {
        let re: f64 = StandardNormal.sample(rng);
        let im: f64 = StandardNormal.sample(rng);
        *xi += Complex64::new(re * sigma_n, im * sigma_n);
    }
    x
}

fn raw_pulse_pair_and_power(x: &[Complex64]) -> (f64, f64, f64) {
    let (v1, v2) = staggered_pulse_pair_velocities(x, WAVELENGTH_M, T1, T2);
    let even: Vec<Complex64> = x.iter().step_by(2).copied().collect();
    let odd: Vec<Complex64> = x.iter().skip(1).step_by(2).copied().collect();
    let n_even = lamula_noise::noise_floor_estimate(&even);
    let n_odd = lamula_noise::noise_floor_estimate(&odd);
    let s_even = (mean_power(&even) - n_even).max(0.0);
    let s_odd = (mean_power(&odd) - n_odd).max(0.0);
    (v1, v2, 0.5 * (s_even + s_odd))
}

fn mean_power(y: &[Complex64]) -> f64 {
    y.iter().map(|c| c.norm_sqr()).sum::<f64>() / y.len() as f64
}

fn filtered_pulse_pair_and_power(x: &[Complex64]) -> (f64, f64, f64) {
    let out = lamula_staggered_prt::sz2000_clutter_filter(x, HALF_WIDTH_BINS);
    let (v1, v2) = staggered_pulse_pair_velocities(&out.filtered, WAVELENGTH_M, T1, T2);
    let z_hat = reflectivity_estimate(x, HALF_WIDTH_BINS);
    (v1, v2, z_hat)
}

fn summarize(
    power_clutter: f64,
    mean_v: f64,
    sigma_v: f64,
    apply_filter: bool,
    n_trials: usize,
    rng: &mut StdRng,
) -> (f64, f64, f64) {
    let mut v1_sum = 0.0;
    let mut v2_sum = 0.0;
    let mut z_sum = 0.0;
    for _ in 0..n_trials {
        let x = generate_staggered_cell_with_clutter(mean_v, sigma_v, power_clutter, M_PULSES, rng);
        let (v1, v2, z) = if apply_filter {
            filtered_pulse_pair_and_power(&x)
        } else {
            raw_pulse_pair_and_power(&x)
        };
        v1_sum += v1;
        v2_sum += v2;
        z_sum += z;
    }
    let n = n_trials as f64;
    (v1_sum / n, v2_sum / n, z_sum / n)
}

#[test]
fn safe_velocity_far_from_notch_band() {
    let bin_spacing_sub = 2.0 * v_a_sub() / (M_PULSES / 2) as f64;
    let band = HALF_WIDTH_BINS as f64 * bin_spacing_sub;
    assert!(
        V_SAFE_MPS > 5.0 * band,
        "V_SAFE_MPS debe quedar lejos de la banda de notch: banda=+-{band:.3} m/s"
    );
}

/// Prueba 1 del oráculo: clutter 20 dB sobre la señal, meteoro lejos de la
/// banda de notch. Sin filtrar, el clutter sesga Z más del 50%; filtrado,
/// Z se recupera dentro del 25% y v1/v2 dentro de 1.0 m/s, tan cerca de la
/// verdad como el caso sin clutter (margen 2x).
#[test]
fn strong_clutter_recovered_far_from_notch_band() {
    const N_TRIALS: usize = 150;
    let mut rng = StdRng::seed_from_u64(20260913);

    let (v1_clean, v2_clean, _) =
        summarize(0.0, V_SAFE_MPS, SIGMA_V_TEST, false, N_TRIALS, &mut rng);
    let (_, _, z_raw) = summarize(100.0, V_SAFE_MPS, SIGMA_V_TEST, false, N_TRIALS, &mut rng);
    let (v1_filt, v2_filt, z_filt) =
        summarize(100.0, V_SAFE_MPS, SIGMA_V_TEST, true, N_TRIALS, &mut rng);

    assert!(
        (z_raw - POWER_WEATHER).abs() > 0.50 * POWER_WEATHER,
        "sin filtrar, el clutter debería sesgar Z en más del 50%: Z_hat={z_raw:.4}"
    );
    assert!(
        (z_filt - POWER_WEATHER).abs() < 0.25 * POWER_WEATHER,
        "filtrado, Z debería recuperarse dentro del 25%: Z_hat={z_filt:.4}"
    );
    assert!(
        (v1_filt - V_SAFE_MPS).abs() < 1.0,
        "v1 filtrado fuera de tolerancia: v1_hat={v1_filt:.3}"
    );
    assert!(
        (v2_filt - V_SAFE_MPS).abs() < 1.0,
        "v2 filtrado fuera de tolerancia: v2_hat={v2_filt:.3}"
    );
    assert!(
        (v1_filt - V_SAFE_MPS).abs() < 2.0 * (v1_clean - V_SAFE_MPS).abs().max(0.05),
        "v1 filtrado debería estar tan cerca de la verdad como el caso sin clutter"
    );
    assert!(
        (v2_filt - V_SAFE_MPS).abs() < 2.0 * (v2_clean - V_SAFE_MPS).abs().max(0.05),
        "v2 filtrado debería estar tan cerca de la verdad como el caso sin clutter"
    );
}

/// Prueba 2 del oráculo: curva frente a razón clutter/señal (0-40 dB), Z
/// dentro del 25% y v1/v2 dentro de 1.0 m/s en todo el barrido.
#[test]
fn reflectivity_and_velocity_track_truth_across_csr_curve() {
    const N_TRIALS: usize = 100;
    const CSR_DB_GRID: [f64; 5] = [0.0, 10.0, 20.0, 30.0, 40.0];
    let mut rng = StdRng::seed_from_u64(20260914);

    for &csr_db in &CSR_DB_GRID {
        let power_clutter = POWER_WEATHER * 10f64.powf(csr_db / 10.0);
        let (v1, v2, z) = summarize(
            power_clutter,
            V_SAFE_MPS,
            SIGMA_V_TEST,
            true,
            N_TRIALS,
            &mut rng,
        );
        assert!(
            (z - POWER_WEATHER).abs() < 0.25 * POWER_WEATHER,
            "CSR={csr_db}dB: Z fuera de tolerancia, Z_hat={z:.4}"
        );
        assert!(
            (v1 - V_SAFE_MPS).abs() < 1.0 && (v2 - V_SAFE_MPS).abs() < 1.0,
            "CSR={csr_db}dB: v1/v2 fuera de tolerancia, v1={v1:.3} v2={v2:.3}"
        );
    }
}

/// Prueba 3 del oráculo: sin clutter, filtro siempre activo. Lejos de la
/// banda, degradación de Z < 20%. Dentro de la banda (velocidad verdadera
/// cero), el notch pierde una fracción medible de Z — limitación conocida
/// y declarada, no oculta.
#[test]
fn filter_degrades_minimally_far_from_band_and_measurably_inside_it() {
    const N_TRIALS: usize = 150;
    let mut rng = StdRng::seed_from_u64(20260915);

    let (_, _, z_raw_far) = summarize(0.0, V_SAFE_MPS, SIGMA_V_TEST, false, N_TRIALS, &mut rng);
    let (_, _, z_filt_far) = summarize(0.0, V_SAFE_MPS, SIGMA_V_TEST, true, N_TRIALS, &mut rng);
    assert!(
        (z_filt_far - z_raw_far).abs() < 0.20 * POWER_WEATHER,
        "lejos de la banda, el filtro no debería degradar Z más del 20%: raw={z_raw_far:.4} filt={z_filt_far:.4}"
    );

    let (_, _, z_raw_in) = summarize(0.0, 0.0, SIGMA_V_TEST, false, N_TRIALS, &mut rng);
    let (_, _, z_filt_in) = summarize(0.0, 0.0, SIGMA_V_TEST, true, N_TRIALS, &mut rng);
    let loss_frac = (z_raw_in - z_filt_in) / z_raw_in;
    assert!(
        loss_frac > 0.20,
        "dentro de la banda, el notch debería perder una fracción medible de Z \
         (limitación conocida): pérdida={loss_frac:.2}"
    );
}
