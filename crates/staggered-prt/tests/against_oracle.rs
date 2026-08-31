//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/staggered_prt.ipynb`.
//! Reproduce sus tolerancias exactas con un número de realizaciones
//! recortado para mantener el test rápido. El generador de I/Q es propio de
//! este oráculo (no el del simulador general): el muestreo escalonado no es
//! uniforme, así que no hay una FFT ordinaria detrás -- se genera
//! directamente en el dominio del tiempo por Cholesky de la matriz de
//! covarianza `C[i,j] = R(t_i - t_j)`, la misma ACF gaussiana cerrada de
//! Doviak & Zrnić evaluada en los tiempos de muestreo reales.

#![allow(clippy::needless_range_loop)]

use lamula_dual_prf::dealias_dual_prf;
use lamula_staggered_prt::staggered_pulse_pair_velocities;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;

const WAVELENGTH_M: f64 = 0.10;
const T1: f64 = 0.8e-3;
const T2: f64 = 1.2e-3; // razón 2:3
const M_PULSES: usize = 32;
const NOISE_FLOOR: f64 = 0.05;

fn v_a1() -> f64 {
    WAVELENGTH_M / (4.0 * T1)
}
fn v_a2() -> f64 {
    WAVELENGTH_M / (4.0 * T2)
}
fn v_ext() -> f64 {
    2.0 * v_a1()
}

/// `R(tau)` cerrada de Doviak & Zrnić (1993) cap. 6, evaluada en retardos
/// arbitrarios (no sólo múltiplos de un PRT uniforme).
fn analytic_acf(power_s: f64, mean_v: f64, sigma_v: f64, wavelength_m: f64, tau: f64) -> Complex64 {
    let phase = Complex64::from_polar(
        1.0,
        4.0 * std::f64::consts::PI * mean_v * tau / wavelength_m,
    );
    let decay = (-8.0 * std::f64::consts::PI.powi(2) * sigma_v.powi(2) * tau.powi(2)
        / wavelength_m.powi(2))
    .exp();
    phase * (power_s * decay)
}

fn staggered_times(m: usize, t1: f64, t2: f64) -> Vec<f64> {
    let mut times = vec![0.0f64; m];
    for i in 1..m {
        let dt = if (i - 1) % 2 == 0 { t1 } else { t2 };
        times[i] = times[i - 1] + dt;
    }
    times
}

/// Cholesky de una matriz hermítica definida positiva `m x m`: `C = L·L^H`
/// con `L` triangular inferior.
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

#[allow(clippy::too_many_arguments)]
fn generate_staggered_cell(
    power_s: f64,
    mean_v: f64,
    sigma_v: f64,
    wavelength_m: f64,
    t1: f64,
    t2: f64,
    m: usize,
    noise_floor: f64,
    rng: &mut StdRng,
) -> Vec<Complex64> {
    let times = staggered_times(m, t1, t2);
    let mut c = vec![vec![Complex64::new(0.0, 0.0); m]; m];
    for i in 0..m {
        for j in 0..m {
            let tau = times[i] - times[j];
            c[i][j] = analytic_acf(power_s, mean_v, sigma_v, wavelength_m, tau);
        }
    }
    // Hermitizar (redondeo de punto flotante) y regularizar la diagonal,
    // igual que el oráculo.
    for i in 0..m {
        for j in 0..m {
            let avg = (c[i][j] + c[j][i].conj()) / 2.0;
            c[i][j] = avg;
        }
        c[i][i] += Complex64::new(1e-9 * power_s, 0.0);
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

    if noise_floor > 0.0 {
        let sigma = (noise_floor / 2.0).sqrt();
        for xi in x.iter_mut() {
            let re: f64 = StandardNormal.sample(rng);
            let im: f64 = StandardNormal.sample(rng);
            *xi += Complex64::new(re * sigma, im * sigma);
        }
    }
    x
}

fn fold(v: f64, v_a: f64) -> f64 {
    ((v + v_a).rem_euclid(2.0 * v_a)) - v_a
}

fn dealias_trial(v_true: f64, sigma_v: f64, snr_db: f64, rng: &mut StdRng) -> f64 {
    let power_s = NOISE_FLOOR * 10f64.powf(snr_db / 10.0);
    let x = generate_staggered_cell(
        power_s,
        v_true,
        sigma_v,
        WAVELENGTH_M,
        T1,
        T2,
        M_PULSES,
        NOISE_FLOOR,
        rng,
    );
    let (v1, v2) = staggered_pulse_pair_velocities(&x, WAVELENGTH_M, T1, T2);
    dealias_dual_prf(v1, v2, v_a1(), v_a2(), v_ext()).velocity_mps
}

fn is_hit(v_hat: f64, v_true: f64) -> bool {
    (v_hat - v_true).abs() < v_a2()
}

#[test]
fn fold_helper_matches_dual_prf_convention() {
    assert!((fold(0.0, 10.0) - 0.0).abs() < 1e-9);
    assert!((fold(15.0, 10.0) - (-5.0)).abs() < 1e-9);
}

/// Prueba 1 del oráculo: malla (SNR, σv) en la zona segura de velocidad
/// (fuera de la degeneración estructural cerca del borde de `v_ext`, misma
/// razón 2:3 que el dual-PRF). A SNR alta la tasa de acierto es >= 95% en
/// todo el barrido de σv, no cae al subir la SNR (degradación suave) y a
/// SNR muy baja se degrada medible y claramente.
#[test]
fn hit_rate_grid_in_safe_zone_degrades_smoothly() {
    const SNR_DB_GRID: [f64; 6] = [-5.0, 0.0, 5.0, 10.0, 15.0, 20.0];
    const SIGMA_V_GRID: [f64; 4] = [0.5, 1.5, 3.0, 6.0];
    const N_TRIALS: usize = 200;

    let v_safe = 0.4 * v_ext();
    let mut rng = StdRng::seed_from_u64(20260912);

    let mut hit_rates = std::collections::HashMap::new();
    for &snr_db in &SNR_DB_GRID {
        for &sigma_v in &SIGMA_V_GRID {
            let hits = (0..N_TRIALS)
                .filter(|_| is_hit(dealias_trial(v_safe, sigma_v, snr_db, &mut rng), v_safe))
                .count();
            hit_rates.insert(
                (snr_db.to_bits(), sigma_v.to_bits()),
                hits as f64 / N_TRIALS as f64,
            );
        }
    }

    for &sigma_v in &SIGMA_V_GRID {
        let rate_high = hit_rates[&(20.0f64.to_bits(), sigma_v.to_bits())];
        assert!(
            rate_high >= 0.95,
            "sv={sigma_v}: tasa de acierto a SNR=20dB={rate_high:.3} por debajo de 0.95"
        );

        let series: Vec<f64> = SNR_DB_GRID
            .iter()
            .map(|&snr| hit_rates[&(snr.to_bits(), sigma_v.to_bits())])
            .collect();
        for i in 0..series.len() - 1 {
            assert!(
                series[i] <= series[i + 1] + 0.05,
                "sv={sigma_v}: la tasa de acierto cae al subir la SNR entre {} y {} dB ({:.3} -> {:.3})",
                SNR_DB_GRID[i],
                SNR_DB_GRID[i + 1],
                series[i],
                series[i + 1]
            );
        }

        let rate_low = hit_rates[&((-5.0f64).to_bits(), sigma_v.to_bits())];
        assert!(
            rate_low < 0.90,
            "sv={sigma_v}: tasa de acierto a SNR=-5dB={rate_low:.3} no se degrada lo suficiente"
        );
    }
}
