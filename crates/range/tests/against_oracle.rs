//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra
//! `tools/oracles/procesamiento_de_rango.ipynb`. Reproduce sus tres
//! escenarios y tolerancias exactas (celdas 4, 6 y 8).

use lamula_noise::total_power;
use lamula_range::{assign_range_gate, average_power, compose_split_cut};
use lamula_simulator::{generate_cell, CellParams};
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use rustfft::num_complex::Complex64;
use std::f64::consts::PI;

/// Celda 4 del oráculo: blanco puntual en rango conocido, asignado dentro de
/// media celda en todas las combinaciones de ancho de pulso / cell_mode.
#[test]
fn point_target_assigned_within_half_cell() {
    const START_RANGE_M: f64 = 250.0;
    const N_GATES: u32 = 400;
    let fine_spacings_m = [62.5, 125.0, 250.0]; // pulse_width_idx -> resolución física
    let cell_mode_ks = [1u32, 2, 4];

    let mut rng = StdRng::seed_from_u64(20260903);
    let worst_half_cell = fine_spacings_m.iter().cloned().fold(0.0, f64::max)
        * *cell_mode_ks.iter().max().unwrap() as f64
        / 2.0;

    let mut max_error = 0.0f64;
    for &fine_spacing in &fine_spacings_m {
        for &k in &cell_mode_ks {
            let coarse_spacing = fine_spacing * k as f64;
            let max_range = START_RANGE_M + N_GATES as f64 * coarse_spacing;
            for _ in 0..500 {
                let r_true = rng.gen_range(START_RANGE_M..max_range);
                let (_, center) =
                    assign_range_gate(r_true, START_RANGE_M, fine_spacing, k, N_GATES)
                        .expect("blanco dentro de alcance mal rechazado");
                max_error = max_error.max((center - r_true).abs());
            }
        }
    }

    assert!(
        max_error < worst_half_cell,
        "error máximo de asignación {max_error:.3} m excede media celda más gruesa {worst_half_cell:.1} m"
    );

    assert!(assign_range_gate(START_RANGE_M - 10.0, START_RANGE_M, 125.0, 1, N_GATES).is_none());
    assert!(assign_range_gate(
        START_RANGE_M + 125.0 * N_GATES as f64 + 10.0,
        START_RANGE_M,
        125.0,
        1,
        N_GATES
    )
    .is_none());
}

/// Celda 6 del oráculo: promediar `K` estimaciones independientes de potencia
/// de celda reduce la varianza en el factor `K` teórico, dentro de 10%.
#[test]
fn averaging_reduces_variance_by_theoretical_factor() {
    const POWER_S: f64 = 1.0;
    const NOISE_FLOOR: f64 = 0.01;
    const MEAN_V: f64 = 5.0;
    const SIGMA_V: f64 = 1.5;
    const WAVELENGTH_M: f64 = 0.10;
    const PRT_S: f64 = 1.0e-3;
    const M: usize = 64;
    const N_TRIALS: usize = 4000;
    const VARIANCE_RATIO_TOLERANCE: f64 = 0.10;

    let params = CellParams {
        power_s: POWER_S,
        mean_v: MEAN_V,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: M,
        noise_floor: NOISE_FLOOR,
    };

    let mut rng = StdRng::seed_from_u64(20260903);
    let sample_gate_power = |rng: &mut StdRng| total_power(&generate_cell(&params, rng));

    let base_estimates: Vec<f64> = (0..N_TRIALS).map(|_| sample_gate_power(&mut rng)).collect();
    let var_k1 = sample_variance(&base_estimates);

    for &k in &[2usize, 4, 8, 16] {
        let mut averaged = Vec::with_capacity(N_TRIALS);
        for _ in 0..N_TRIALS {
            let draws: Vec<f64> = (0..k).map(|_| sample_gate_power(&mut rng)).collect();
            averaged.push(average_power(&draws));
        }
        let var_k = sample_variance(&averaged);
        let ratio = var_k1 / var_k;

        assert!(
            (ratio - k as f64).abs() < VARIANCE_RATIO_TOLERANCE * k as f64,
            "K={k}: ratio var(K=1)/var(K)={ratio:.3} fuera de {VARIANCE_RATIO_TOLERANCE:.0} del factor teórico {k}"
        );
    }
}

fn sample_variance(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)
}

/// Estimador de pulso-pareado estándar (Doviak & Zrnić 1993, cap. 6),
/// reproducido aquí sólo como herramienta para demostrar la composición
/// split-cut, igual que hace `tools/oracles/procesamiento_de_rango.ipynb` —
/// su propio oráculo (y su implementación Rust) viven en
/// `docs/algorithms/pulse-pair-moments.md`, no en esta página.
fn pulse_pair_velocity(y: &[Complex64], wavelength_m: f64, prt_s: f64) -> f64 {
    let mut r1 = Complex64::new(0.0, 0.0);
    for w in y.windows(2) {
        r1 += w[0] * w[1].conj();
    }
    r1 /= (y.len() - 1) as f64;
    -wavelength_m / (4.0 * PI * prt_s) * r1.arg()
}

/// Celda 8 del oráculo: composición split-cut. Z del barrido de PRF baja
/// dentro de 2% de la verdad-terreno; V del barrido de PRF alta dentro de
/// 0.5 m/s; V del barrido de PRF baja muestra el alias esperado, no la
/// verdad-terreno.
#[test]
fn split_cut_composes_reflectivity_low_prf_velocity_high_prf() {
    const POWER_S: f64 = 1.0;
    const NOISE_FLOOR: f64 = 0.01;
    const SIGMA_V: f64 = 1.5;
    const WAVELENGTH_M: f64 = 0.10;
    const M: usize = 64;
    const PRT_LOW: f64 = 2.0e-3;
    const PRT_HIGH: f64 = 0.5e-3;
    const V_TRUE: f64 = 20.0;
    const N_TRIALS: usize = 1000;

    let v_a_low = WAVELENGTH_M / (4.0 * PRT_LOW);
    let v_low_expected_alias = V_TRUE - 2.0 * v_a_low;

    let mut rng = StdRng::seed_from_u64(20260903);
    let mut z_ests = Vec::with_capacity(N_TRIALS);
    let mut v_high_ests = Vec::with_capacity(N_TRIALS);
    let mut v_low_ests = Vec::with_capacity(N_TRIALS);

    for _ in 0..N_TRIALS {
        let params_low = CellParams {
            power_s: POWER_S,
            mean_v: V_TRUE,
            sigma_v: SIGMA_V,
            wavelength_m: WAVELENGTH_M,
            prt_s: PRT_LOW,
            m: M,
            noise_floor: NOISE_FLOOR,
        };
        let params_high = CellParams {
            prt_s: PRT_HIGH,
            ..params_low
        };

        let y_low = generate_cell(&params_low, &mut rng);
        let y_high = generate_cell(&params_high, &mut rng);

        // Z aquí es R(0) sin resta de ruido, igual que el oráculo — esta
        // página no reestima ruido, eso es competencia de ruido-y-umbrales.
        z_ests.push(total_power(&y_low));
        v_high_ests.push(pulse_pair_velocity(&y_high, WAVELENGTH_M, PRT_HIGH));
        v_low_ests.push(pulse_pair_velocity(&y_low, WAVELENGTH_M, PRT_LOW));

        // La composición en sí: seleccionar Z del barrido bajo y V del alto.
        let _ = compose_split_cut(*z_ests.last().unwrap(), *v_high_ests.last().unwrap());
    }

    let z_mean = z_ests.iter().sum::<f64>() / N_TRIALS as f64;
    let v_high_mean = v_high_ests.iter().sum::<f64>() / N_TRIALS as f64;
    let v_low_mean = v_low_ests.iter().sum::<f64>() / N_TRIALS as f64;

    assert!(
        (z_mean - POWER_S).abs() < 0.02 * POWER_S,
        "Z publicada (PRF baja) {z_mean:.4} fuera de 2% de la verdad-terreno {POWER_S}"
    );
    assert!(
        (v_high_mean - V_TRUE).abs() < 0.5,
        "V publicada (PRF alta) {v_high_mean:.3} fuera de 0.5 m/s de la verdad-terreno {V_TRUE}"
    );
    assert!(
        (v_low_mean - v_low_expected_alias).abs() < 0.5 && (v_low_mean - V_TRUE).abs() > 5.0,
        "V del barrido PRF baja {v_low_mean:.3} no muestra el alias esperado {v_low_expected_alias:.3}"
    );
}
