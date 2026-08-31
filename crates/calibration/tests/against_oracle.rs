//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra
//! `tools/oracles/reflectivity_calibration.ipynb`. Reproduce sus escenarios y
//! tolerancias exactas (celdas 6 y 8): corrección por r² a lo largo de todo
//! el alcance, y linealidad en todo el rango dinámico. La potencia de señal
//! se separa de ruido con `lamula_noise` (misma cadena que
//! `docs/algorithms/ruido-y-umbrales.md`), no reimplementada aquí.

use lamula_calibration::{dbz_to_power, power_to_dbz};
use lamula_noise::{noise_floor_estimate, subtract_noise, total_power};
use lamula_simulator::{generate_cell, CellParams};
use rand::rngs::StdRng;
use rand::SeedableRng;

const RADAR_CONSTANT_DB: f64 = -40.0;
const NOISE_FLOOR: f64 = 0.05;
const MEAN_V: f64 = 5.0;
const SIGMA_V: f64 = 1.5;
const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const M: usize = 256;

/// `S_hat = max(R(0) - N_HS74, 0)` de una única ráfaga simulada, o `None` si
/// la celda se censura por falta de señal detectable.
fn estimate_linear_power(power_s: f64, rng: &mut StdRng) -> Option<f64> {
    let params = CellParams {
        power_s,
        mean_v: MEAN_V,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: M,
        noise_floor: NOISE_FLOOR,
    };
    let y = generate_cell(&params, rng);
    let r0_hat = total_power(&y);
    let n_hat = noise_floor_estimate(&y);
    subtract_noise(r0_hat, n_hat)
}

/// Celda 6 del oráculo: blanco fijo a 30 dBZ, barrido en rango. La Z
/// recuperada (tras corrección por r²) debe quedar dentro de 0.5 dB a lo
/// largo de todo el alcance 5-200 km.
#[test]
fn range_correction_recovers_fixed_target_across_range() {
    const Z_TRUE: f64 = 30.0;
    const N_TRIALS: usize = 800;
    const RANGE_BIAS_TOLERANCE_DB: f64 = 0.5;

    let mut rng = StdRng::seed_from_u64(20260902);

    for &range_km in &[5.0, 20.0, 50.0, 100.0, 150.0, 200.0] {
        let power_s = dbz_to_power(Z_TRUE, range_km, RADAR_CONSTANT_DB);

        let mean_dbz: f64 = (0..N_TRIALS)
            .map(|_| {
                let s_hat = estimate_linear_power(power_s, &mut rng)
                    .expect("SNR alta a 30 dBZ, la celda no debería censurarse");
                power_to_dbz(s_hat, range_km, RADAR_CONSTANT_DB)
            })
            .sum::<f64>()
            / N_TRIALS as f64;

        let bias_db = mean_dbz - Z_TRUE;
        assert!(
            bias_db.abs() < RANGE_BIAS_TOLERANCE_DB,
            "range={range_km} km: sesgo={bias_db:.3} dB excede tolerancia {RANGE_BIAS_TOLERANCE_DB} dB"
        );
    }
}

/// Celda 8 del oráculo: linealidad en todo el rango dinámico a 100 km.
/// Ajuste sobre SNR>=0 dB: pendiente dentro de 0.02 de 1.0, residuo máximo
/// respecto de la recta teórica < 0.5 dB.
#[test]
fn linearity_across_dynamic_range() {
    const RANGE_KM: f64 = 100.0;
    const N_TRIALS: usize = 800;
    const SLOPE_TOLERANCE: f64 = 0.02;
    const RESIDUAL_TOLERANCE_DB: f64 = 0.5;

    let mut rng = StdRng::seed_from_u64(20260902);

    let snr_db_grid: Vec<f64> = {
        let mut v = Vec::new();
        let mut snr = -6.0;
        while snr < 42.0 {
            v.push(snr);
            snr += 4.0;
        }
        v
    };

    let mut fit_true = Vec::new();
    let mut fit_hat = Vec::new();

    for &snr_db in &snr_db_grid {
        let power_s = NOISE_FLOOR * 10f64.powf(snr_db / 10.0);
        let dbz_true = power_to_dbz(power_s, RANGE_KM, RADAR_CONSTANT_DB);

        let mut estimates = Vec::with_capacity(N_TRIALS);
        for _ in 0..N_TRIALS {
            if let Some(s_hat) = estimate_linear_power(power_s, &mut rng) {
                estimates.push(power_to_dbz(s_hat, RANGE_KM, RADAR_CONSTANT_DB));
            }
        }
        if estimates.is_empty() {
            continue;
        }
        let dbz_hat = estimates.iter().sum::<f64>() / estimates.len() as f64;

        if snr_db >= 0.0 {
            fit_true.push(dbz_true);
            fit_hat.push(dbz_hat);
        }
    }

    // Regresión lineal (mínimos cuadrados) igual que `np.polyfit(..., 1)`.
    let n = fit_true.len() as f64;
    let mean_x = fit_true.iter().sum::<f64>() / n;
    let mean_y = fit_hat.iter().sum::<f64>() / n;
    let cov_xy: f64 = fit_true
        .iter()
        .zip(&fit_hat)
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum();
    let var_x: f64 = fit_true.iter().map(|x| (x - mean_x).powi(2)).sum();
    let slope = cov_xy / var_x;
    let intercept = mean_y - slope * mean_x;

    let max_residual = fit_true
        .iter()
        .zip(&fit_hat)
        .map(|(x, y)| (y - (slope * x + intercept)).abs())
        .fold(0.0f64, f64::max);

    assert!(
        (slope - 1.0).abs() < SLOPE_TOLERANCE,
        "pendiente de linealidad {slope:.4} fuera de {SLOPE_TOLERANCE} de 1.0"
    );
    assert!(
        max_residual < RESIDUAL_TOLERANCE_DB,
        "residuo máximo {max_residual:.3} dB excede tolerancia {RESIDUAL_TOLERANCE_DB} dB"
    );
}
