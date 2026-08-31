//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/kdp_estimacion.ipynb`.
//! Reproduce sus tolerancias exactas con una malla y un número de
//! realizaciones recortados para mantener el test rápido. El contraste
//! contra Py-ART que la página pide queda pendiente, tal como el propio
//! oráculo lo declara fuera de alcance (sin Py-ART disponible).
//!
//! El ruido de fase no se añade a mano: cada celda del perfil se mide
//! simulando de verdad la covarianza cruzada H/V con `generate_dual_pol_cell`
//! del simulador, censura previa por ρHV asumida ya hecha aguas arriba
//! (dependencia con `lamula-polarimetry`, ver el módulo del crate).

use lamula_kdp::{kdp_window_fit, unwrap_deg};
use lamula_polarimetry::polarimetric_moments_simultaneous;
use lamula_simulator::{generate_dual_pol_cell, CellParams, DualPolParams};
use rand::rngs::StdRng;
use rand::SeedableRng;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const NOISE_FLOOR: f64 = 0.05;
const POWER_H: f64 = 1.0;
const MEAN_V: f64 = 5.0;
const SIGMA_V: f64 = 1.5;
const GATE_SPACING_KM: f64 = 0.150;
const N_GATES: usize = 200;
const WINDOW_GATES: usize = 15;

fn measure_phidp(rho_hv: f64, m: usize, phidp_true_deg: f64, rng: &mut StdRng) -> f64 {
    let params = CellParams {
        power_s: POWER_H,
        mean_v: MEAN_V,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m,
        noise_floor: NOISE_FLOOR,
    };
    let dual = DualPolParams {
        zdr_db: 1.0,
        rho_hv,
        phidp_deg: phidp_true_deg,
    };
    let (yh, yv) = generate_dual_pol_cell(&params, &dual, rng);
    let est = polarimetric_moments_simultaneous(&yh, &yv, 0.0, 0.0, 0.0);
    est.phidp_deg
}

/// Celda `simulate_phidp_profile` del oráculo: integra `kdp_true_profile`
/// (grados/km) para obtener ΦDP verdadero, mide cada celda y desdobla.
fn simulate_phidp_profile(
    kdp_true_profile: &[f64],
    rho_hv: f64,
    m: usize,
    rng: &mut StdRng,
) -> Vec<f64> {
    let mut phidp_true = Vec::with_capacity(kdp_true_profile.len());
    let mut acc = 0.0;
    for &k in kdp_true_profile {
        acc += k;
        phidp_true.push(2.0 * acc * GATE_SPACING_KM);
    }
    let measured: Vec<f64> = phidp_true
        .iter()
        .map(|&phi| measure_phidp(rho_hv, m, phi, rng))
        .collect();
    unwrap_deg(&measured)
}

/// Prueba 1 — sesgo sobre KDP constante en una malla recortada de (ρHV, M).
#[test]
fn constant_kdp_bias_across_grid() {
    let mut rng = StdRng::seed_from_u64(20260917);
    const K0: f64 = 2.0;
    const BIAS_TOLERANCE: f64 = 0.3;
    let edge = WINDOW_GATES;

    for &rho_hv in &[0.99f64, 0.90] {
        for &m in &[32usize, 128] {
            let n_realizations = 8;
            let mut estimates = Vec::with_capacity(n_realizations);
            for _ in 0..n_realizations {
                let kdp_true_profile = vec![K0; N_GATES];
                let phidp_unwrapped =
                    simulate_phidp_profile(&kdp_true_profile, rho_hv, m, &mut rng);
                let kdp_hat = kdp_window_fit(&phidp_unwrapped, GATE_SPACING_KM, WINDOW_GATES);
                let interior = &kdp_hat[edge..N_GATES - edge];
                let valid: Vec<f64> = interior.iter().filter_map(|k| *k).collect();
                let mean = valid.iter().sum::<f64>() / valid.len() as f64;
                estimates.push(mean);
            }
            let mean_est = estimates.iter().sum::<f64>() / estimates.len() as f64;
            let bias = mean_est - K0;
            assert!(
                bias.abs() < BIAS_TOLERANCE,
                "rho_hv={rho_hv} M={m} bias={bias}"
            );
        }
    }
}

/// Prueba 2 — escalón: el cruce del 50% cae dentro del ancho de la ventana.
#[test]
fn step_crossing_within_window_width() {
    let mut rng = StdRng::seed_from_u64(20260918);
    const STEP_INDEX: usize = 100;
    const K0_STEP: f64 = 3.0;
    let n_realizations = 12;

    let kdp_true_step: Vec<f64> = (0..N_GATES)
        .map(|i| if i < STEP_INDEX { 0.0 } else { K0_STEP })
        .collect();

    let mut sum = vec![0.0f64; N_GATES];
    let mut count = vec![0usize; N_GATES];
    for _ in 0..n_realizations {
        let phidp_unwrapped = simulate_phidp_profile(&kdp_true_step, 0.97, 64, &mut rng);
        let kdp_hat = kdp_window_fit(&phidp_unwrapped, GATE_SPACING_KM, WINDOW_GATES);
        for (i, k) in kdp_hat.iter().enumerate() {
            if let Some(k) = k {
                sum[i] += k;
                count[i] += 1;
            }
        }
    }
    let mean: Vec<f64> = sum
        .iter()
        .zip(count.iter())
        .map(|(&s, &c)| if c > 0 { s / c as f64 } else { f64::NAN })
        .collect();

    let lo = STEP_INDEX - 20;
    let hi = STEP_INDEX + 20;
    let crossing_idx = (lo..hi).find(|&i| mean[i] > K0_STEP / 2.0).unwrap_or(hi);
    let crossing_offset = crossing_idx.abs_diff(STEP_INDEX);
    assert!(
        crossing_offset <= WINDOW_GATES,
        "crossing_offset={crossing_offset}"
    );
}

/// Prueba 3 — tramo nulo: sin sesgo negativo sistemático ni oscilación
/// excesiva.
#[test]
fn null_kdp_has_no_systematic_bias_or_excess_noise() {
    let mut rng = StdRng::seed_from_u64(20260919);
    const NULL_BIAS_TOLERANCE: f64 = 0.05;
    const NULL_STD_TOLERANCE: f64 = 1.2;
    let n_realizations = 12;
    let edge = WINDOW_GATES;

    let mut all_values = Vec::new();
    for _ in 0..n_realizations {
        let kdp_true_null = vec![0.0f64; N_GATES];
        let phidp_unwrapped = simulate_phidp_profile(&kdp_true_null, 0.97, 64, &mut rng);
        let kdp_hat = kdp_window_fit(&phidp_unwrapped, GATE_SPACING_KM, WINDOW_GATES);
        all_values.extend(kdp_hat[edge..N_GATES - edge].iter().filter_map(|k| *k));
    }
    let n = all_values.len() as f64;
    let mean = all_values.iter().sum::<f64>() / n;
    let variance = all_values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std = variance.sqrt();

    assert!(mean.abs() < NULL_BIAS_TOLERANCE, "mean={mean}");
    assert!(std < NULL_STD_TOLERANCE, "std={std}");
}
