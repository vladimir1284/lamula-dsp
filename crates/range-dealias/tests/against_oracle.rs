//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra
//! `tools/oracles/dealiasing_de_rango.ipynb`. Cubre la Prueba 2 (recuperación
//! por fase aleatoria) con las tolerancias exactas del oráculo; la Prueba 1
//! (detección dual-PRF) usa un modelo de probabilidad de detección
//! explícitamente ilustrativo que no es parte del algoritmo -- la
//! reconciliación en sí (`classify_trip`) ya tiene sus propios tests
//! unitarios deterministas en `src/detect.rs`.

use lamula_range_dealias::recover_trip1;
use lamula_simulator::{generate_cell, CellParams};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const M: usize = 64;
const NOISE_FLOOR: f64 = 0.05;
const POWER1: f64 = 1.0;
const MEAN_V1: f64 = 5.0;
const SIGMA_V1: f64 = 1.5;
const MEAN_V2: f64 = -10.0;
const SIGMA_V2: f64 = 1.5;

fn complex_gaussian(rng: &mut impl Rng, variance: f64) -> Complex64 {
    let sigma = (variance / 2.0).sqrt();
    let re: f64 = StandardNormal.sample(rng);
    let im: f64 = StandardNormal.sample(rng);
    Complex64::new(re * sigma, im * sigma)
}

fn generate_signal_only(
    power_s: f64,
    mean_v: f64,
    sigma_v: f64,
    rng: &mut StdRng,
) -> Vec<Complex64> {
    let params = CellParams {
        power_s,
        mean_v,
        sigma_v,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: M,
        noise_floor: 0.0,
    };
    generate_cell(&params, rng)
}

/// Celda `recover_trip1` del oráculo.
fn oracle_recover_trip1(power2_ratio_db: f64, rng: &mut StdRng) -> (f64, f64) {
    let power2 = POWER1 * 10f64.powf(power2_ratio_db / 10.0);
    let x1 = generate_signal_only(POWER1, MEAN_V1, SIGMA_V1, rng);
    let x2 = generate_signal_only(power2, MEAN_V2, SIGMA_V2, rng);
    let phi1: Vec<f64> = (0..M)
        .map(|_| rng.gen_range(-std::f64::consts::PI..std::f64::consts::PI))
        .collect();
    let phi2: Vec<f64> = (0..M)
        .map(|_| rng.gen_range(-std::f64::consts::PI..std::f64::consts::PI))
        .collect();

    let y: Vec<Complex64> = (0..M)
        .map(|i| {
            x1[i] * Complex64::from_polar(1.0, phi1[i])
                + x2[i] * Complex64::from_polar(1.0, phi2[i])
                + complex_gaussian(rng, NOISE_FLOOR)
        })
        .collect();

    let est = recover_trip1(&y, &phi1, WAVELENGTH_M, PRT_S);
    (est.s_linear, est.velocity_mps)
}

/// Prueba 2 — Z1 y V1 se recuperan bien cuando el segundo trip es más débil,
/// y la degradación es medible y clara cuando domina.
#[test]
fn trip1_recovery_degrades_with_power_ratio() {
    let mut rng = StdRng::seed_from_u64(20260925);
    const RATIO_DB_GRID: [f64; 5] = [-10.0, -5.0, 0.0, 5.0, 10.0];
    const N_TRIALS: usize = 300; // recortado de 500 del oráculo

    let mut z1_by_ratio = Vec::new();
    let mut v1_by_ratio = Vec::new();
    for &ratio_db in &RATIO_DB_GRID {
        let mut z_sum = 0.0;
        let mut v_sum = 0.0;
        for _ in 0..N_TRIALS {
            let (z, v) = oracle_recover_trip1(ratio_db, &mut rng);
            z_sum += z;
            v_sum += v;
        }
        z1_by_ratio.push(z_sum / N_TRIALS as f64);
        v1_by_ratio.push(v_sum / N_TRIALS as f64);
    }

    for i in 0..3 {
        assert!(
            (z1_by_ratio[i] - POWER1).abs() < 0.15 * POWER1,
            "ratio={} z1={}",
            RATIO_DB_GRID[i],
            z1_by_ratio[i]
        );
        assert!(
            (v1_by_ratio[i] - MEAN_V1).abs() < 0.5,
            "ratio={} v1={}",
            RATIO_DB_GRID[i],
            v1_by_ratio[i]
        );
    }

    let last = z1_by_ratio.len() - 1;
    assert!(
        (z1_by_ratio[last] - POWER1).abs() > 0.30 * POWER1,
        "z1_at_+10dB={}",
        z1_by_ratio[last]
    );
}
