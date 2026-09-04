//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra
//! `tools/oracles/polarimetria_covarianzas.ipynb`. Reproduce sus tolerancias
//! exactas con una malla y un número de realizaciones recortados para
//! mantener el test rápido.
//!
//! El generador de modo alternante y de LDR es propio de este test (no el
//! del simulador general): igual que en `staggered-prt`, el muestreo no
//! simultáneo entre H y V no tiene una FFT ordinaria detrás -- se genera
//! directamente en el dominio del tiempo por Cholesky de la matriz de
//! covarianza `C[i,j] = R(t_i - t_j)` (celda `generate_alternating_dualpol`
//! del oráculo). El modo simultáneo sí reutiliza `lamula_simulator`, que ya
//! lo soporta.

#![allow(clippy::needless_range_loop)]

use lamula_polarimetry::{
    ldr_db, polarimetric_moments_alternating, polarimetric_moments_simultaneous, PolarimetricFlag,
};
use lamula_simulator::{generate_dual_pol_cell, CellParams, DualPolParams};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const MEAN_V: f64 = 5.0;
const SIGMA_V: f64 = 1.5;
const POWER_H: f64 = 1.0;
const NOISE_FLOOR: f64 = 0.05;
const ZDR_TRUE: f64 = 2.0;
const PHIDP_TRUE: f64 = 30.0;

const BIAS_TOL_ZDR_DB: f64 = 0.3;
const BIAS_TOL_RHO: f64 = 0.03;
const BIAS_TOL_PHIDP: f64 = 3.0;

fn complex_gaussian(rng: &mut impl Rng, variance: f64) -> Complex64 {
    let sigma = (variance / 2.0).sqrt();
    let re: f64 = StandardNormal.sample(rng);
    let im: f64 = StandardNormal.sample(rng);
    Complex64::new(re * sigma, im * sigma)
}

/// Celda "Modo simultáneo": ZDR/ρHV/ΦDP sin sesgo apreciable en una malla
/// recortada de `(M, ρHV verdadero)`.
#[test]
fn simultaneous_bias_across_grid() {
    let mut rng = StdRng::seed_from_u64(20260913);
    for &m in &[32usize, 128] {
        for &rho_true in &[0.99f64, 0.80] {
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
                zdr_db: ZDR_TRUE,
                rho_hv: rho_true,
                phidp_deg: PHIDP_TRUE,
            };
            let n_trials = 150;
            let (mut zdr_sum, mut rho_sum, mut phidp_sum, mut n_ok) = (0.0, 0.0, 0.0, 0usize);
            for _ in 0..n_trials {
                let (yh, yv) = generate_dual_pol_cell(&params, &dual, &mut rng);
                let est = polarimetric_moments_simultaneous(&yh, &yv, 0.0, 0.0, 0.05);
                if est.flag == PolarimetricFlag::Ok {
                    zdr_sum += est.zdr_db;
                    rho_sum += est.rhohv;
                    phidp_sum += est.phidp_deg;
                    n_ok += 1;
                }
            }
            assert!(n_ok > n_trials / 2, "demasiadas celdas censuradas");
            let n_ok = n_ok as f64;
            let zdr_bias = zdr_sum / n_ok - ZDR_TRUE;
            let rho_bias = rho_sum / n_ok - rho_true;
            let phidp_bias = phidp_sum / n_ok - PHIDP_TRUE;
            assert!(
                zdr_bias.abs() < BIAS_TOL_ZDR_DB,
                "M={m} rho={rho_true} zdr_bias={zdr_bias}"
            );
            assert!(
                rho_bias.abs() < BIAS_TOL_RHO,
                "M={m} rho={rho_true} rho_bias={rho_bias}"
            );
            assert!(
                phidp_bias.abs() < BIAS_TOL_PHIDP,
                "M={m} rho={rho_true} phidp_bias={phidp_bias}"
            );
        }
    }
}

/// Celda "Barrido de SNR": ρHV se mantiene plano tras la resta de ruido por
/// canal hasta SNR=5 dB.
#[test]
fn rhohv_flat_across_snr_after_noise_subtraction() {
    let mut rng = StdRng::seed_from_u64(20260914);
    const RHO_TRUE: f64 = 0.97;
    for &snr_db in &[20.0f64, 10.0, 5.0] {
        let power_h = NOISE_FLOOR * 10f64.powf(snr_db / 10.0);
        let params = CellParams {
            power_s: power_h,
            mean_v: MEAN_V,
            sigma_v: SIGMA_V,
            wavelength_m: WAVELENGTH_M,
            prt_s: PRT_S,
            m: 64,
            noise_floor: NOISE_FLOOR,
        };
        let dual = DualPolParams {
            zdr_db: ZDR_TRUE,
            rho_hv: RHO_TRUE,
            phidp_deg: PHIDP_TRUE,
        };
        let n_trials = 150;
        let mut rho_sum = 0.0;
        let mut n_ok = 0usize;
        for _ in 0..n_trials {
            let (yh, yv) = generate_dual_pol_cell(&params, &dual, &mut rng);
            let est = polarimetric_moments_simultaneous(&yh, &yv, 0.0, 0.0, 0.05);
            if est.flag == PolarimetricFlag::Ok {
                rho_sum += est.rhohv;
                n_ok += 1;
            }
        }
        assert!(n_ok > n_trials / 2, "demasiadas celdas censuradas");
        let rho_mean = rho_sum / n_ok as f64;
        assert!(
            (rho_mean - RHO_TRUE).abs() < 0.05,
            "SNR={snr_db} rho_mean={rho_mean}"
        );
    }
}

/// `R(tau)` cerrada de Doviak & Zrnić (1993) cap. 6, la misma que en
/// `staggered-prt`.
fn analytic_acf(sigma_v: f64, wavelength_m: f64, tau: f64) -> Complex64 {
    let phase = Complex64::from_polar(
        1.0,
        4.0 * std::f64::consts::PI * MEAN_V * tau / wavelength_m,
    );
    let decay = (-8.0 * std::f64::consts::PI.powi(2) * sigma_v.powi(2) * tau.powi(2)
        / wavelength_m.powi(2))
    .exp();
    phase * decay
}

/// Cholesky de una matriz hermítica definida positiva `n x n`.
fn cholesky(c: &[Vec<Complex64>], n: usize) -> Vec<Vec<Complex64>> {
    let mut l = vec![vec![Complex64::new(0.0, 0.0); n]; n];
    for i in 0..n {
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

/// Celda `generate_alternating_dualpol` del oráculo: secuencia combinada a
/// paso `t_step`, H en pulsos pares y V en impares -- retardo `t_step` (medio
/// PRT de canal) entre la muestra H y la V de cada par.
#[allow(clippy::too_many_arguments)]
fn generate_alternating_dualpol(
    power_h: f64,
    sigma_v: f64,
    wavelength_m: f64,
    t_step: f64,
    m_per_channel: usize,
    zdr_db: f64,
    rho_hv: f64,
    phidp_deg: f64,
    noise_floor: f64,
    rng: &mut impl Rng,
) -> (Vec<Complex64>, Vec<Complex64>) {
    let n_total = 2 * m_per_channel;
    let times: Vec<f64> = (0..n_total).map(|i| i as f64 * t_step).collect();
    let mut c = vec![vec![Complex64::new(0.0, 0.0); n_total]; n_total];
    for i in 0..n_total {
        for j in 0..n_total {
            c[i][j] = analytic_acf(sigma_v, wavelength_m, times[i] - times[j]);
        }
    }
    for i in 0..n_total {
        c[i][i] += Complex64::new(1e-9, 0.0);
    }
    let l = cholesky(&c, n_total);

    let z1: Vec<Complex64> = (0..n_total).map(|_| complex_gaussian(rng, 1.0)).collect();
    let z2: Vec<Complex64> = (0..n_total).map(|_| complex_gaussian(rng, 1.0)).collect();
    let mut w1 = vec![Complex64::new(0.0, 0.0); n_total];
    let mut w2 = vec![Complex64::new(0.0, 0.0); n_total];
    for i in 0..n_total {
        for j in 0..=i {
            w1[i] += l[i][j] * z1[j];
            w2[i] += l[i][j] * z2[j];
        }
    }

    let power_v = power_h / 10f64.powf(zdr_db / 10.0);
    let rho = Complex64::from_polar(rho_hv, phidp_deg.to_radians());
    let l21 = rho.conj();
    let l22 = (1.0 - rho_hv * rho_hv).sqrt();

    let x_h_full: Vec<Complex64> = w1.iter().map(|w| w * power_h.sqrt()).collect();
    let x_v_full: Vec<Complex64> = w1
        .iter()
        .zip(w2.iter())
        .map(|(a, b)| (l21 * a + l22 * b) * power_v.sqrt())
        .collect();

    let mut h_meas: Vec<Complex64> = x_h_full.iter().step_by(2).copied().collect();
    let mut v_meas: Vec<Complex64> = x_v_full.iter().skip(1).step_by(2).copied().collect();
    if noise_floor > 0.0 {
        for x in h_meas.iter_mut() {
            *x += complex_gaussian(rng, noise_floor);
        }
        for x in v_meas.iter_mut() {
            *x += complex_gaussian(rng, noise_floor);
        }
    }
    (h_meas, v_meas)
}

/// Celda "Estimador por celda": el estimador de ρHV corregido por
/// decorrelación queda dentro de 0.03 del ρHV verdadero en la malla de `M`.
#[test]
fn alternating_rhohv_corrected_matches_truth() {
    let mut rng = StdRng::seed_from_u64(20260915);
    const T_STEP: f64 = 1.0e-3;
    const RHO_TRUE_ALT: f64 = 0.97;
    for &m in &[48usize, 96] {
        let n_trials = 80;
        let mut rho_sum = 0.0;
        let mut n_ok = 0usize;
        for _ in 0..n_trials {
            let (h, v) = generate_alternating_dualpol(
                POWER_H,
                SIGMA_V,
                WAVELENGTH_M,
                T_STEP,
                m,
                ZDR_TRUE,
                RHO_TRUE_ALT,
                PHIDP_TRUE,
                NOISE_FLOOR,
                &mut rng,
            );
            let est = polarimetric_moments_alternating(
                &h,
                &v,
                0.0,
                0.0,
                SIGMA_V,
                MEAN_V,
                WAVELENGTH_M,
                T_STEP,
                0.05,
            );
            if est.flag == PolarimetricFlag::Ok {
                rho_sum += est.rhohv;
                n_ok += 1;
            }
        }
        assert!(n_ok > n_trials / 2, "demasiadas celdas censuradas");
        let rho_mean = rho_sum / n_ok as f64;
        assert!(
            (rho_mean - RHO_TRUE_ALT).abs() < BIAS_TOL_RHO,
            "M={m} rho_mean={rho_mean}"
        );
    }
}

/// Celda "Modo alternante" del oráculo, parte ΦDP: el término de fase
/// Doppler de `arg(R_hv)` a retardo `t_step` es real (bias grande, ~sesgo
/// esperado `-4π·MEAN_V·T_STEP/λ` en grados) y `polarimetric_moments_alternating`
/// debe recuperar `PHIDP_TRUE` una vez corregido con `velocity_mps=MEAN_V`.
#[test]
fn alternating_phidp_corrected_matches_truth_naive_is_biased() {
    let mut rng = StdRng::seed_from_u64(20260917);
    const T_STEP: f64 = 1.0e-3;
    const RHO_TRUE_ALT: f64 = 0.97;
    for &m in &[48usize, 96] {
        let n_trials = 80;
        let (mut phidp_corrected_sum, mut phidp_naive_sum) = (0.0, 0.0);
        let mut n_ok = 0usize;
        for _ in 0..n_trials {
            let (h, v) = generate_alternating_dualpol(
                POWER_H,
                SIGMA_V,
                WAVELENGTH_M,
                T_STEP,
                m,
                ZDR_TRUE,
                RHO_TRUE_ALT,
                PHIDP_TRUE,
                NOISE_FLOOR,
                &mut rng,
            );
            let corrected = polarimetric_moments_alternating(
                &h,
                &v,
                0.0,
                0.0,
                SIGMA_V,
                MEAN_V,
                WAVELENGTH_M,
                T_STEP,
                0.05,
            );
            // Naive: mismo estimador con `velocity_mps=0.0`, equivalente a no
            // corregir el término de fase Doppler -- reproduce el error
            // clásico de aplicar la fórmula simultánea sin más en este modo.
            let naive = polarimetric_moments_alternating(
                &h,
                &v,
                0.0,
                0.0,
                SIGMA_V,
                0.0,
                WAVELENGTH_M,
                T_STEP,
                0.05,
            );
            if corrected.flag == PolarimetricFlag::Ok && naive.flag == PolarimetricFlag::Ok {
                phidp_corrected_sum += corrected.phidp_deg;
                phidp_naive_sum += naive.phidp_deg;
                n_ok += 1;
            }
        }
        assert!(n_ok > n_trials / 2, "demasiadas celdas censuradas");
        let n_ok = n_ok as f64;
        let corrected_bias = phidp_corrected_sum / n_ok - PHIDP_TRUE;
        let naive_bias = phidp_naive_sum / n_ok - PHIDP_TRUE;
        assert!(
            corrected_bias.abs() < BIAS_TOL_PHIDP,
            "M={m} corrected_bias={corrected_bias}"
        );
        // El sesgo Doppler esperado a estos parámetros es grande (~36°, ver
        // `doppler_phase_rad` en el oráculo) -- muy por encima de
        // `BIAS_TOL_PHIDP`, confirma que el efecto es real y no un artefacto
        // de muestra finita.
        assert!(
            naive_bias.abs() > 10.0 * BIAS_TOL_PHIDP,
            "M={m} naive_bias={naive_bias}"
        );
    }
}

/// Celda "LDR": sigue la verdad-terreno muy por encima del aislamiento y se
/// satura cerca de él muy por debajo.
///
/// A diferencia del oráculo -- que genera `hh`/`vh` como tonos gaussianos
/// blancos y resta un `NOISE_FLOOR` constante conocido -- este test genera
/// las dos celdas con la misma forma Doppler que el resto del pipeline
/// (`generate_cell`), porque `ldr_db` resta ruido con el estimador HS74
/// espectral (`lamula_noise`), que necesita una forma espectral no plana
/// para separar señal de ruido; sobre un tono blanco literal, HS74
/// interpreta toda la potencia como ruido. La fuga de aislamiento se suma en
/// potencia a la señal cruzada verdadera, como en el oráculo.
#[test]
fn ldr_tracks_truth_above_isolation_and_saturates_below() {
    let mut rng = StdRng::seed_from_u64(20260916);
    const ANTENNA_ISOLATION_DB: f64 = 30.0;
    const P_HH: f64 = 1000.0;
    let leakage_power = P_HH * 10f64.powf(-ANTENNA_ISOLATION_DB / 10.0);

    let measure_ldr = |ldr_true_db: f64, rng: &mut StdRng| -> f64 {
        let p_vh_true = P_HH * 10f64.powf(ldr_true_db / 10.0);
        let hh_params = CellParams {
            power_s: P_HH,
            mean_v: MEAN_V,
            sigma_v: SIGMA_V,
            wavelength_m: WAVELENGTH_M,
            prt_s: PRT_S,
            m: 64,
            noise_floor: NOISE_FLOOR,
        };
        let vh_params = CellParams {
            power_s: p_vh_true + leakage_power,
            ..hh_params
        };
        let n_trials = 100;
        let mut sum = 0.0;
        for _ in 0..n_trials {
            let hh = lamula_simulator::generate_cell(&hh_params, rng);
            let vh = lamula_simulator::generate_cell(&vh_params, rng);
            let est = ldr_db(&hh, &vh, ANTENNA_ISOLATION_DB);
            sum += est.ldr_db;
        }
        sum / n_trials as f64
    };

    let strong = measure_ldr(-10.0, &mut rng);
    assert!((strong - (-10.0)).abs() < 1.0, "strong={strong}");

    let weak = measure_ldr(-50.0, &mut rng);
    assert!((weak - (-ANTENNA_ISOLATION_DB)).abs() < 3.0, "weak={weak}");
    assert!((weak - (-50.0)).abs() > 10.0, "weak={weak}");

    // M grande para que una única celda tenga varianza baja: la fiabilidad es
    // una propiedad de una estimación puntual, no de un promedio de Monte
    // Carlo como `weak` arriba.
    let weak_hh_params = CellParams {
        power_s: P_HH,
        mean_v: MEAN_V,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: 1024,
        noise_floor: NOISE_FLOOR,
    };
    let weak_vh_params = CellParams {
        power_s: P_HH * 10f64.powf(-50.0 / 10.0) + leakage_power,
        ..weak_hh_params
    };
    let weak_hh = lamula_simulator::generate_cell(&weak_hh_params, &mut rng);
    let weak_vh = lamula_simulator::generate_cell(&weak_vh_params, &mut rng);
    let est = ldr_db(&weak_hh, &weak_vh, ANTENNA_ISOLATION_DB);
    assert!(
        !est.reliable,
        "LDR muy por debajo del aislamiento debe marcarse no fiable (ldr_db={})",
        est.ldr_db
    );
}
