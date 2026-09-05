//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra
//! `tools/oracles/sz_second_trip_recovery.ipynb`. Reproduce sus tolerancias
//! exactas (Pruebas 2 y 3) con un número de realizaciones recortado para
//! mantener el test rápido; la Prueba 1 (estructura espectral del código) ya
//! tiene su propio test unitario determinista en `crates/sz864/src/code.rs`.

use lamula_simulator::{generate_cell, CellParams};
use lamula_sz864::{separate_trips, sz_8_64_phases};
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
const V_NYQUIST: f64 = WAVELENGTH_M / (4.0 * PRT_S);
const NOTCH_WIDTH_MPS: f64 = 2.0 * V_NYQUIST / 8.0; // 1/8 del intervalo de Nyquist

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

/// Combina dos trips bajo el código `psi_code` (transmitido) tal como llega
/// el eco: el trip fuerte con la fase del pulso actual, el débil con la del
/// pulso anterior (viajó un PRT más) — mismo celda `recover_trip1_sz` /
/// `recover_trip1_nocode` del oráculo, aquí unificada porque
/// [`separate_trips`] ya decodifica internamente.
fn mixed_echo(power2_ratio_db: f64, rng: &mut StdRng, psi_code: &[f64]) -> Vec<Complex64> {
    let power2 = POWER1 * 10f64.powf(power2_ratio_db / 10.0);
    let x1 = generate_signal_only(POWER1, MEAN_V1, SIGMA_V1, rng);
    let x2 = generate_signal_only(power2, MEAN_V2, SIGMA_V2, rng);
    let m = psi_code.len();
    (0..m)
        .map(|k| {
            let psi_prev = psi_code[(k + m - 1) % m];
            x1[k] * Complex64::from_polar(1.0, psi_code[k])
                + x2[k] * Complex64::from_polar(1.0, psi_prev)
                + complex_gaussian(rng, NOISE_FLOOR)
        })
        .collect()
}

/// Prueba 2 del oráculo: con código, V1 se mantiene dentro de 1 m/s de la
/// verdad en todo el barrido -10..10 dB; sin código (código nulo, mismo
/// truco que el oráculo), el error supera 2 m/s en cuanto el segundo trip
/// iguala o supera al primero.
#[test]
fn strong_trip_velocity_bias_with_vs_without_code() {
    let mut rng = StdRng::seed_from_u64(20260904);
    let (psi, phi) = sz_8_64_phases(M);
    let zero_code = vec![0.0; M];

    const RATIO_DB_GRID: [f64; 5] = [-10.0, -5.0, 0.0, 5.0, 10.0];
    const N_TRIALS: usize = 300; // recortado de 600 del oráculo

    let mut v1_sz = Vec::new();
    let mut v1_nocode = Vec::new();
    for &ratio_db in &RATIO_DB_GRID {
        let mut sum_sz = 0.0;
        let mut sum_no = 0.0;
        for _ in 0..N_TRIALS {
            let y_sz = mixed_echo(ratio_db, &mut rng, &psi);
            sum_sz += separate_trips(&y_sz, &psi, &phi, WAVELENGTH_M, PRT_S, NOTCH_WIDTH_MPS)
                .strong
                .velocity_mps;

            let y_no = mixed_echo(ratio_db, &mut rng, &zero_code);
            sum_no += separate_trips(
                &y_no,
                &zero_code,
                &zero_code,
                WAVELENGTH_M,
                PRT_S,
                NOTCH_WIDTH_MPS,
            )
            .strong
            .velocity_mps;
        }
        v1_sz.push(sum_sz / N_TRIALS as f64);
        v1_nocode.push(sum_no / N_TRIALS as f64);
    }

    for (i, &ratio_db) in RATIO_DB_GRID.iter().enumerate() {
        assert!(
            (v1_sz[i] - MEAN_V1).abs() < 1.0,
            "ratio={} v1_sz={}",
            ratio_db,
            v1_sz[i]
        );
        if ratio_db >= 0.0 {
            assert!(
                (v1_nocode[i] - MEAN_V1).abs() > 2.0,
                "ratio={} v1_nocode={}",
                ratio_db,
                v1_nocode[i]
            );
        }
    }
}

/// Prueba 3 del oráculo: el trip débil se recupera con sesgo < 1 m/s y
/// desviación < 2 m/s a razones moderadas (-5 y -10 dB), y la degradación es
/// clara y medible a razón extrema (-20 dB, desviación > 5 m/s).
#[test]
fn weak_trip_recovery_degrades_with_power_ratio() {
    let mut rng = StdRng::seed_from_u64(20260904);
    let (psi, phi) = sz_8_64_phases(M);

    const RATIO_DB_GRID: [f64; 4] = [-5.0, -10.0, -15.0, -20.0];
    const N_TRIALS: usize = 250; // recortado de 500 del oráculo

    let mut v2_mean = Vec::new();
    let mut v2_std = Vec::new();
    for &ratio_db in &RATIO_DB_GRID {
        let vs: Vec<f64> = (0..N_TRIALS)
            .map(|_| {
                let y = mixed_echo(ratio_db, &mut rng, &psi);
                separate_trips(&y, &psi, &phi, WAVELENGTH_M, PRT_S, NOTCH_WIDTH_MPS)
                    .weak
                    .velocity_mps
            })
            .collect();
        let mean = vs.iter().sum::<f64>() / N_TRIALS as f64;
        let var = vs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / N_TRIALS as f64;
        v2_mean.push(mean);
        v2_std.push(var.sqrt());
    }

    for i in 0..2 {
        assert!(
            (v2_mean[i] - MEAN_V2).abs() < 1.0 && v2_std[i] < 2.0,
            "ratio={} mean={} std={}",
            RATIO_DB_GRID[i],
            v2_mean[i],
            v2_std[i]
        );
    }
    let last = v2_std.len() - 1;
    assert!(
        v2_std[last] > 5.0,
        "ratio={} std={}",
        RATIO_DB_GRID[last],
        v2_std[last]
    );
}
