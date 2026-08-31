//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra
//! `tools/oracles/calibracion_polarimetrica.ipynb`. Reproduce sus
//! tolerancias exactas (recorta sólo el número de celdas/trials donde el
//! oráculo usa cifras muy altas, para mantener el test rápido).
//!
//! La prueba 1 del oráculo ("aplicar el offset conocido recupera la
//! verdad-terreno") ya está cubierta por
//! `lamula_polarimetry::covariance::tests::offsets_subtract_from_zdr_and_phidp`
//! — es la misma resta, ya probada donde vive el código que la hace. Este
//! archivo cubre las pruebas 2 y 3, que son las que tienen contenido propio
//! de este crate.

use lamula_pol_calibration::{phidp_system_offset_deg, zdr_offset_from_birdbath_db};
use lamula_polarimetry::polarimetric_moments_simultaneous;
use lamula_simulator::{generate_dual_pol_cell, CellParams, DualPolParams};
use rand::rngs::StdRng;
use rand::SeedableRng;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const NOISE_FLOOR: f64 = 0.05;
const POWER_H: f64 = 1.0;
const SIGMA_V: f64 = 1.5;
const RHO_HV: f64 = 0.97;

/// Celda `measure_zdr` del oráculo: un desbalance de ganancia H/V se suma
/// directamente al ZDR visto por el receptor.
fn measure_zdr(
    mean_v: f64,
    m: usize,
    zdr_true_scene: f64,
    gain_imbalance_db: f64,
    rng: &mut StdRng,
) -> f64 {
    let params = CellParams {
        power_s: POWER_H,
        mean_v,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m,
        noise_floor: NOISE_FLOOR,
    };
    let dual = DualPolParams {
        zdr_db: zdr_true_scene + gain_imbalance_db,
        rho_hv: RHO_HV,
        phidp_deg: 0.0,
    };
    let (yh, yv) = generate_dual_pol_cell(&params, &dual, rng);
    polarimetric_moments_simultaneous(&yh, &yv, 0.0, 0.0, 0.0).zdr_db
}

/// Celda `measure_phidp` del oráculo.
fn measure_phidp(m: usize, phidp_true_deg: f64, rng: &mut StdRng) -> f64 {
    let params = CellParams {
        power_s: POWER_H,
        mean_v: 5.0,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m,
        noise_floor: NOISE_FLOOR,
    };
    let dual = DualPolParams {
        zdr_db: 1.0,
        rho_hv: RHO_HV,
        phidp_deg: phidp_true_deg,
    };
    let (yh, yv) = generate_dual_pol_cell(&params, &dual, rng);
    polarimetric_moments_simultaneous(&yh, &yv, 0.0, 0.0, 0.0).phidp_deg
}

/// Prueba 2 — birdbath: el procedimiento recupera el desbalance inyectado
/// dentro de 0.1 dB.
#[test]
fn birdbath_recovers_injected_gain_imbalance() {
    let mut rng = StdRng::seed_from_u64(20260920);
    const GAIN_IMBALANCE_BIRDBATH: f64 = -0.35;
    const N_GATES_BIRDBATH: usize = 100;
    const BIRDBATH_TOLERANCE_DB: f64 = 0.1;

    let measurements: Vec<f64> = (0..N_GATES_BIRDBATH)
        .map(|_| measure_zdr(0.0, 64, 0.0, GAIN_IMBALANCE_BIRDBATH, &mut rng))
        .filter(|z| !z.is_nan())
        .collect();

    let offset_hat = zdr_offset_from_birdbath_db(&measurements);
    assert!(
        (offset_hat - GAIN_IMBALANCE_BIRDBATH).abs() < BIRDBATH_TOLERANCE_DB,
        "offset_hat={offset_hat}"
    );
}

/// Prueba 3 — ΦDP de sistema: separa el offset de equipo de la fase de
/// propagación dentro de 2°.
#[test]
fn phidp_system_separates_from_propagation_phase() {
    let mut rng = StdRng::seed_from_u64(20260921);
    const PHIDP_SYSTEM_TRUE: f64 = 15.0;
    const N_GATES_PHIDP: usize = 150;
    const GATE_SPACING_KM: f64 = 0.150;
    const K0: f64 = 2.0;
    const PROP_START_IDX: usize = 40;
    const FIRST_GATES: usize = 20;
    const PHIDP_TOLERANCE_DEG: f64 = 2.0;

    let mut acc = 0.0;
    let phidp_true_profile: Vec<f64> = (0..N_GATES_PHIDP)
        .map(|i| {
            let kdp = if i < PROP_START_IDX { 0.0 } else { K0 };
            acc += kdp;
            PHIDP_SYSTEM_TRUE + 2.0 * acc * GATE_SPACING_KM
        })
        .collect();

    let phidp_measured: Vec<f64> = phidp_true_profile
        .iter()
        .map(|&phi| measure_phidp(64, phi, &mut rng))
        .collect();

    let phidp_system_hat = phidp_system_offset_deg(&phidp_measured, FIRST_GATES);
    assert!(
        (phidp_system_hat - PHIDP_SYSTEM_TRUE).abs() < PHIDP_TOLERANCE_DEG,
        "phidp_system_hat={phidp_system_hat}"
    );
}
