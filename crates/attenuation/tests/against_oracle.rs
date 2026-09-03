//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/atenuacion_zphi.ipynb`.
//! Reproduce su escenario "caso de modelo acoplado" con medidas simuladas de
//! verdad -- potencia vía `lamula_simulator::generate_cell` +
//! `lamula_calibration::power_to_dbz` (misma cadena que
//! `crates/calibration/tests/against_oracle.rs`), perfil de ΦDP completo vía
//! `lamula_simulator::generate_dual_pol_cell` +
//! `lamula_polarimetry::polarimetric_moments_simultaneous` +
//! `lamula_kdp::unwrap_deg` (misma cadena que
//! `crates/kdp/tests/against_oracle.rs::simulate_phidp_profile`) -- en vez de
//! alimentar el perfil "verdadero" directamente a `zphi_correct_dbz`. Medir
//! sólo los dos extremos del tramo sin pasar por el perfil completo NO sirve
//! aquí: el ΔΦDP total de un tramo con atenuación de varios dB por el método
//! de Testud supera de sobra los ±180° que un único par de medidas puede
//! resolver sin ambigüedad -- exactamente la razón por la que
//! `lamula_kdp::unwrap_deg` existe.

use lamula_attenuation::zphi_correct_dbz;
use lamula_calibration::{dbz_to_power, power_to_dbz};
use lamula_kdp::unwrap_deg;
use lamula_noise::{noise_floor_estimate, subtract_noise, total_power};
use lamula_polarimetry::polarimetric_moments_simultaneous;
use lamula_simulator::{generate_cell, generate_dual_pol_cell, CellParams, DualPolParams};
use rand::rngs::StdRng;
use rand::SeedableRng;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const NOISE_FLOOR: f64 = 0.05;
const MEAN_V: f64 = 5.0;
const SIGMA_V: f64 = 1.5;
const M: usize = 128;
const RADAR_CONSTANT_DB: f64 = -20.0;
const DR_KM: f64 = 0.5;
const N_GATES: usize = 60;
const BETA: f64 = 0.64884;
const A_COEF_C_BAND: f64 = 0.08;
const RHO_HV_TRUE: f64 = 0.98;
const START_RANGE_KM: f64 = 5.0;

fn bell_profile_dbz(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let r = i as f64 * DR_KM;
            25.0 + 25.0 * (-0.5 * ((r - 15.0) / 6.0f64).powi(2)).exp()
        })
        .collect()
}

/// Potencia lineal media medida sobre `n_trials` ráfagas simuladas a
/// `range_km` con la ecuación del radar ya invertida (celda 6 de
/// `tools/oracles/reflectivity_calibration.ipynb`, misma cadena).
fn measure_dbz(true_dbz: f64, range_km: f64, n_trials: usize, rng: &mut StdRng) -> f64 {
    let power_s = dbz_to_power(true_dbz, range_km, RADAR_CONSTANT_DB);
    let params = CellParams {
        power_s,
        mean_v: MEAN_V,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: M,
        noise_floor: NOISE_FLOOR,
    };
    let mean_power: f64 = (0..n_trials)
        .filter_map(|_| {
            let y = generate_cell(&params, rng);
            let r0_hat = total_power(&y);
            let n_hat = noise_floor_estimate(&y);
            subtract_noise(r0_hat, n_hat)
        })
        .sum::<f64>()
        / n_trials as f64;
    power_to_dbz(mean_power, range_km, RADAR_CONSTANT_DB)
}

/// Una realización del perfil de ΦDP desdoblado: mide cada celda una sola
/// vez (`phidp_true_profile_deg` es el ángulo verdadero acumulado en esa
/// celda) y desdobla el perfil COMPLETO -- promediar medidas ya desdobladas
/// celda a celda, en cambio, escondería el problema real del método (un
/// salto entre celdas ADYACENTES nunca supera 180°, así que
/// `lamula_kdp::unwrap_deg` sólo funciona sobre la secuencia completa, no
/// sobre dos puntos aislados del tramo).
fn simulate_phidp_profile(phidp_true_profile_deg: &[f64], rng: &mut StdRng) -> Vec<f64> {
    let params = CellParams {
        power_s: 1.0,
        mean_v: MEAN_V,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: M,
        noise_floor: NOISE_FLOOR,
    };
    let measured: Vec<f64> = phidp_true_profile_deg
        .iter()
        .map(|&phi| {
            let dual = DualPolParams {
                zdr_db: 1.0,
                rho_hv: RHO_HV_TRUE,
                phidp_deg: phi,
            };
            let (yh, yv) = generate_dual_pol_cell(&params, &dual, rng);
            polarimetric_moments_simultaneous(&yh, &yv, 0.0, 0.0, 0.0).phidp_deg
        })
        .collect();
    unwrap_deg(&measured)
}

/// Caso de modelo acoplado (celda homónima del oráculo): la atenuación
/// "verdadera" se genera con la MISMA relación A=alpha_zA*Z^beta que asume
/// `zphi_correct_dbz` (beta), y el perfil de ΦDP verdadero es el que implica
/// esa atenuación vía `a_coef` (KDP_true = A_true/a_coef) -- exactamente el
/// caso para el que el método es exacto salvo error de discretización y
/// ruido de medida. Promediado sobre varias realizaciones de ΦDP (la
/// potencia, en cambio, ya se promedia dentro de `measure_dbz`: no depende
/// del signo de una fase que puede desdoblarse distinto cada vez) para leer
/// el sesgo del método en vez del ruido de una sola realización.
#[test]
fn matched_model_recovers_true_profile_from_simulated_measurements() {
    let mut rng = StdRng::seed_from_u64(20260902);
    const BIAS_TOLERANCE_DB: f64 = 1.0;
    const N_TRIALS_POWER: usize = 300;
    const N_REALIZATIONS_PHIDP: usize = 20;

    let z_true_dbz = bell_profile_dbz(N_GATES);
    let alpha_za = 0.0006;
    let a_true: Vec<f64> = z_true_dbz
        .iter()
        .map(|&dbz| alpha_za * 10f64.powf(dbz / 10.0).powf(BETA))
        .collect();
    let mut cum_a_true = vec![0.0; N_GATES];
    for i in 1..N_GATES {
        cum_a_true[i] = cum_a_true[i - 1] + 0.5 * (a_true[i - 1] + a_true[i]) * DR_KM;
    }
    let z_attenuated_true_dbz: Vec<f64> = z_true_dbz
        .iter()
        .zip(&cum_a_true)
        .map(|(&z, &c)| z - 2.0 * c)
        .collect();

    let z_meas_dbz: Vec<f64> = z_attenuated_true_dbz
        .iter()
        .enumerate()
        .map(|(i, &dbz)| {
            let range_km = START_RANGE_KM + i as f64 * DR_KM;
            measure_dbz(dbz, range_km, N_TRIALS_POWER, &mut rng)
        })
        .collect();

    // KDP_true = A_true / a_coef (dB/km / (dB/deg) = deg/km); PhiDP_true =
    // 2*integral(KDP_true) -- ver el doc-comment del módulo del crate.
    let kdp_true: Vec<f64> = a_true.iter().map(|&a| a / A_COEF_C_BAND).collect();
    let mut phidp_true = vec![0.0; N_GATES];
    for i in 1..N_GATES {
        phidp_true[i] = phidp_true[i - 1] + (kdp_true[i - 1] + kdp_true[i]) * DR_KM;
    }

    let interior = 5..N_GATES - 5;
    let mut corrected_sum = vec![0.0; N_GATES];
    for _ in 0..N_REALIZATIONS_PHIDP {
        let phidp_unwrapped = simulate_phidp_profile(&phidp_true, &mut rng);
        let delta_phidp_measured = phidp_unwrapped[N_GATES - 1] - phidp_unwrapped[0];
        let corrected = zphi_correct_dbz(
            &z_meas_dbz,
            DR_KM,
            BETA,
            A_COEF_C_BAND,
            delta_phidp_measured,
        );
        for i in 0..N_GATES {
            corrected_sum[i] += corrected[i];
        }
    }
    let corrected_mean: Vec<f64> = corrected_sum
        .iter()
        .map(|&s| s / N_REALIZATIONS_PHIDP as f64)
        .collect();

    let max_bias_corrected = interior
        .clone()
        .map(|i| (corrected_mean[i] - z_true_dbz[i]).abs())
        .fold(0.0f64, f64::max);
    let max_bias_uncorrected = interior
        .map(|i| (z_meas_dbz[i] - z_true_dbz[i]).abs())
        .fold(0.0f64, f64::max);

    assert!(
        max_bias_corrected < BIAS_TOLERANCE_DB,
        "sesgo máximo corregido {max_bias_corrected:.3} dB excede tolerancia {BIAS_TOLERANCE_DB} dB"
    );
    assert!(
        max_bias_corrected < max_bias_uncorrected,
        "la corrección debe reducir el sesgo: corregido={max_bias_corrected:.3} dB, sin corregir={max_bias_uncorrected:.3} dB"
    );
}
