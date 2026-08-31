//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/ruido_y_umbrales.ipynb`.
//! Reproduce sus escenarios y tolerancias exactas (celdas 11 y 13) sobre la
//! implementación Rust; la recuperación del suelo de ruido inyectado sobre
//! ruido puro (celda 9) está en el test unitario
//! `hs74::tests::recovers_injected_noise_floor_on_pure_noise`.

use lamula_noise::{
    censored_by_sig_threshold, noise_floor_estimate, snr_db, subtract_noise, total_power,
};
use lamula_simulator::{generate_cell, CellParams};
use rand::rngs::StdRng;
use rand::SeedableRng;

const NOISE_FLOOR: f64 = 0.3;
const M: usize = 256;
const MEAN_V: f64 = 5.0;
const SIGMA_V: f64 = 1.5;
const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;

fn params(power_s: f64, mean_v: f64, sigma_v: f64) -> CellParams {
    CellParams {
        power_s,
        mean_v,
        sigma_v,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: M,
        noise_floor: NOISE_FLOOR,
    }
}

/// Celda 11 del oráculo: sesgo de `S_hat = max(R(0) - N_HS74, 0)` frente a la
/// SNR de verdad-terreno. Despreciable (< 1 dB) desde SNR = 0 dB en adelante;
/// acotado (< 20 dB) por debajo, donde el criterio de la página es "acotado y
/// documentado", no "insesgado".
#[test]
fn reflectivity_bias_matches_oracle_curve() {
    const N_TRIALS: usize = 2000;
    let mut rng = StdRng::seed_from_u64(20260901);

    for &snr_db_truth in &[-10.0, -5.0, 0.0, 5.0, 10.0, 15.0, 20.0] {
        let power_s = NOISE_FLOOR * 10f64.powf(snr_db_truth / 10.0);
        let p = params(power_s, MEAN_V, SIGMA_V);

        let mut errors_db = Vec::with_capacity(N_TRIALS);
        let mut n_censored = 0usize;
        for _ in 0..N_TRIALS {
            let y = generate_cell(&p, &mut rng);
            let r0_hat = total_power(&y);
            let n_hat = noise_floor_estimate(&y);
            match subtract_noise(r0_hat, n_hat) {
                Some(s_hat) => errors_db.push(10.0 * (s_hat / power_s).log10()),
                None => n_censored += 1,
            }
        }

        let bias_db = errors_db.iter().sum::<f64>() / errors_db.len() as f64;
        let tolerance_db = if snr_db_truth >= 0.0 { 1.0 } else { 20.0 };

        assert!(
            bias_db.abs() < tolerance_db,
            "SNR={snr_db_truth} dB: sesgo={bias_db:.3} dB excede tolerancia {tolerance_db} dB \
             (censuradas={n_censored}/{N_TRIALS})"
        );
    }
}

/// Celda 13 del oráculo: tasa de falsos positivos de `sig_threshold` sobre
/// celdas de ruido puro debe quedar por debajo del 5% declarado.
#[test]
fn false_positive_rate_below_bound() {
    const N_TRIALS: usize = 4000;
    const SIG_THRESHOLD_DB: f64 = 3.0;
    const FP_BOUND: f64 = 0.05;

    let mut rng = StdRng::seed_from_u64(20260901);
    let p = params(0.0, 0.0, 1.0);

    let mut false_positives = 0usize;
    for _ in 0..N_TRIALS {
        let y = generate_cell(&p, &mut rng);
        let r0_hat = total_power(&y);
        let n_hat = noise_floor_estimate(&y);
        let snr = match subtract_noise(r0_hat, n_hat) {
            Some(s_hat) => snr_db(s_hat, NOISE_FLOOR),
            None => f64::NEG_INFINITY,
        };
        if !censored_by_sig_threshold(snr, SIG_THRESHOLD_DB) {
            false_positives += 1;
        }
    }

    let rate = false_positives as f64 / N_TRIALS as f64;
    assert!(
        rate < FP_BOUND,
        "tasa de falsos positivos {rate:.4} excede el límite declarado {FP_BOUND} \
         a sig_threshold={SIG_THRESHOLD_DB} dB"
    );
}
