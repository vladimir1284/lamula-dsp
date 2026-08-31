//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra
//! `tools/oracles/dual_prf_dealiasing.ipynb`. Reproduce sus tolerancias
//! exactas con un número de realizaciones recortado para mantener el test
//! rápido.

use lamula_dual_prf::{continuity_fix, dealias_dual_prf};
use lamula_moments::pulse_pair_moments;
use lamula_simulator::{generate_cell, CellParams};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, Normal};

const WAVELENGTH_M: f64 = 0.10;
const T1: f64 = 1.2e-3;
const T2: f64 = 0.8e-3;
const M_PULSES: usize = 64;
const NOISE_FLOOR: f64 = 0.05;
const SIGMA_V: f64 = 1.5;

fn v_a1() -> f64 {
    WAVELENGTH_M / (4.0 * T1)
}
fn v_a2() -> f64 {
    WAVELENGTH_M / (4.0 * T2)
}
fn v_ext() -> f64 {
    3.0 * v_a1()
}

/// `dealias_trial` del oráculo: estima `v1`/`v2` por pulse-pair sobre cada
/// PRT y desdobla.
fn dealias_trial(v_true1: f64, v_true2: f64, snr_db: f64, rng: &mut impl Rng) -> f64 {
    let power_s = NOISE_FLOOR * 10f64.powf(snr_db / 10.0);
    let params1 = CellParams {
        power_s,
        mean_v: v_true1,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: T1,
        m: M_PULSES,
        noise_floor: NOISE_FLOOR,
    };
    let params2 = CellParams {
        power_s,
        mean_v: v_true2,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: T2,
        m: M_PULSES,
        noise_floor: NOISE_FLOOR,
    };
    let y1 = generate_cell(&params1, rng);
    let y2 = generate_cell(&params2, rng);
    let v1 = pulse_pair_moments(&y1, WAVELENGTH_M, T1).velocity_mps;
    let v2 = pulse_pair_moments(&y2, WAVELENGTH_M, T2).velocity_mps;
    dealias_dual_prf(v1, v2, v_a1(), v_a2(), v_ext()).velocity_mps
}

/// Acierta el número de pliegues -- no sólo "cerca", ver la página: un
/// fallo de desdoblado es un salto de varios múltiplos de la Nyquist.
fn is_hit(v_hat: f64, v_true: f64) -> bool {
    (v_hat - v_true).abs() < v_a1()
}

/// Prueba 1 del oráculo: malla (SNR, velocidad) restringida a
/// `|v| <= 0.6*v_ext`, fuera de la zona con degeneración estructural del
/// esquema 2:3 documentada aparte en el oráculo.
#[test]
fn hit_rate_across_snr_and_velocity_grid() {
    const N_TRIALS: usize = 150;
    const MIN_HIT_RATE: f64 = 0.90;

    let v_ext_val = v_ext();
    let v_grid: [f64; 5] =
        std::array::from_fn(|i| -0.6 * v_ext_val + i as f64 * (1.2 * v_ext_val / 4.0));
    let snr_grid = [0.0, 5.0, 10.0, 15.0, 20.0];

    let mut rng = StdRng::seed_from_u64(20260911);
    for &v_true in &v_grid {
        for &snr_db in &snr_grid {
            let hits = (0..N_TRIALS)
                .filter(|_| is_hit(dealias_trial(v_true, v_true, snr_db, &mut rng), v_true))
                .count();
            let rate = hits as f64 / N_TRIALS as f64;
            assert!(
                rate >= MIN_HIT_RATE,
                "v_true={v_true:.2} SNR={snr_db}: tasa de acierto={rate:.3} por debajo de {MIN_HIT_RATE}"
            );
        }
    }
}

/// Prueba 2 del oráculo: degradación por cizalladura entre los bloques de
/// PRF baja y alta, a SNR alta -- el punto débil estructural del método,
/// no un problema de ruido.
#[test]
fn shear_degrades_hit_rate_monotonically() {
    const V_TRUE: f64 = 25.0;
    const SNR_DB: f64 = 20.0;
    const N_TRIALS: usize = 200;
    const SHEAR_GRID: [f64; 6] = [0.0, 2.0, 5.0, 8.0, 10.0, 15.0];

    let mut rng = StdRng::seed_from_u64(20260911);
    let mut rates = Vec::with_capacity(SHEAR_GRID.len());
    for &shear in &SHEAR_GRID {
        let hits = (0..N_TRIALS)
            .filter(|_| {
                is_hit(
                    dealias_trial(V_TRUE, V_TRUE + shear, SNR_DB, &mut rng),
                    V_TRUE,
                )
            })
            .count();
        rates.push(hits as f64 / N_TRIALS as f64);
    }

    assert!(
        rates[0] >= 0.95,
        "sin cizalladura, tasa de acierto={:.3} por debajo de 0.95",
        rates[0]
    );
    assert!(
        *rates.last().unwrap() < 0.50,
        "con cizalladura fuerte, tasa de acierto={:.3} no se degrada lo suficiente",
        rates.last().unwrap()
    );
    for i in 0..rates.len() - 1 {
        assert!(
            rates[i] >= rates[i + 1] - 0.02,
            "la tasa de acierto sube al aumentar la cizalladura entre shear={} y shear={}: {:.3} -> {:.3}",
            SHEAR_GRID[i],
            SHEAR_GRID[i + 1],
            rates[i],
            rates[i + 1]
        );
    }
}

/// Prueba 3 del oráculo: corrección por continuidad espacial -- recupera la
/// mayoría de los fallos aislados sin estropear los aciertos vecinos.
#[test]
fn continuity_fix_recovers_isolated_failures_without_breaking_hits() {
    const V_TRUE: f64 = 20.0;
    const N_GATES: usize = 60;
    const FAIL_RATE: f64 = 0.15;
    const N_TRIALS: usize = 400;
    const MIN_RECOVERY_RATE: f64 = 0.70;
    const MAX_BREAKAGE_RATE: f64 = 0.20;

    let v_a1_val = v_a1();
    let mut rng = StdRng::seed_from_u64(20260911);
    let mut recovered = 0usize;
    let mut total_failures = 0usize;
    let mut broken = 0usize;
    let mut total_good = 0usize;

    let jitter = Normal::new(0.0, 0.3).unwrap();
    for _ in 0..N_TRIALS {
        let v_hats: Vec<f64> = (0..N_GATES)
            .map(|_| V_TRUE + jitter.sample(&mut rng))
            .collect();
        let is_failed: Vec<bool> = (0..N_GATES).map(|_| rng.gen::<f64>() < FAIL_RATE).collect();
        let fold_choices = [-2.0, -1.0, 1.0, 2.0];
        let v_hats_with_fail: Vec<f64> = (0..N_GATES)
            .map(|i| {
                if is_failed[i] {
                    let k = fold_choices[rng.gen_range(0..fold_choices.len())];
                    v_hats[i] + k * 2.0 * v_a1_val
                } else {
                    v_hats[i]
                }
            })
            .collect();

        let fixed = continuity_fix(&v_hats_with_fail, v_a1_val, 3);

        for i in 0..N_GATES {
            let was_failed = (v_hats_with_fail[i] - V_TRUE).abs() > v_a1_val;
            let still_failed = (fixed[i] - V_TRUE).abs() > v_a1_val;
            if was_failed {
                total_failures += 1;
                if !still_failed {
                    recovered += 1;
                }
            } else {
                total_good += 1;
                if still_failed {
                    broken += 1;
                }
            }
        }
    }

    let recovery_rate = recovered as f64 / total_failures as f64;
    let breakage_rate = broken as f64 / total_good as f64;

    assert!(
        recovery_rate >= MIN_RECOVERY_RATE,
        "tasa de recuperación={recovery_rate:.3} por debajo de {MIN_RECOVERY_RATE}"
    );
    assert!(
        breakage_rate <= MAX_BREAKAGE_RATE,
        "tasa de rotura={breakage_rate:.3} por encima de {MAX_BREAKAGE_RATE}"
    );
}
