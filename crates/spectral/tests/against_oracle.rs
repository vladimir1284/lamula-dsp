//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/estimador_espectral.ipynb`.
//! Reproduce sus tolerancias exactas con un número de realizaciones recortado
//! para mantener el test rápido.

use lamula_simulator::{gaussian_doppler_spectrum, generate_cell, CellParams};
use lamula_spectral::spectral_moments;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const MEAN_V: f64 = 5.0;
const M: usize = 64;

fn complex_gaussian(rng: &mut impl Rng, variance: f64) -> Complex64 {
    let sigma = (variance / 2.0).sqrt();
    let re: f64 = StandardNormal.sample(rng);
    let im: f64 = StandardNormal.sample(rng);
    Complex64::new(re * sigma, im * sigma)
}

fn shape_to_time_domain(
    rng: &mut impl Rng,
    mut spectral_samples: Vec<Complex64>,
    noise_floor: f64,
) -> Vec<Complex64> {
    let mut planner = FftPlanner::new();
    let ifft = planner.plan_fft_inverse(spectral_samples.len());
    ifft.process(&mut spectral_samples);
    if noise_floor > 0.0 {
        for x in spectral_samples.iter_mut() {
            *x += complex_gaussian(rng, noise_floor);
        }
    }
    spectral_samples
}

/// Blanco puntual exacto en un único bin Doppler discreto -- celda
/// `generate_pure_tone_cell` del oráculo.
fn generate_pure_tone_cell(
    power_s: f64,
    bin_index: usize,
    m: usize,
    noise_floor: f64,
    rng: &mut impl Rng,
) -> Vec<Complex64> {
    let mut spectral = vec![Complex64::new(0.0, 0.0); m];
    spectral[bin_index] = complex_gaussian(rng, 1.0) * power_s.sqrt();
    shape_to_time_domain(rng, spectral, noise_floor)
}

/// Suma de dos poblaciones gaussianas de velocidad -- celda
/// `generate_bimodal_cell` del oráculo.
#[allow(clippy::too_many_arguments)]
fn generate_bimodal_cell(
    power1: f64,
    v1: f64,
    sigma1: f64,
    power2: f64,
    v2: f64,
    sigma2: f64,
    wavelength_m: f64,
    prt_s: f64,
    m: usize,
    noise_floor: f64,
    rng: &mut impl Rng,
) -> Vec<Complex64> {
    let spectrum1 = gaussian_doppler_spectrum(power1, v1, sigma1, wavelength_m, prt_s, m);
    let spectrum2 = gaussian_doppler_spectrum(power2, v2, sigma2, wavelength_m, prt_s, m);
    let shaped: Vec<Complex64> = spectrum1
        .iter()
        .zip(&spectrum2)
        .map(|(&s1, &s2)| complex_gaussian(rng, 1.0) * (s1 + s2).sqrt())
        .collect();
    shape_to_time_domain(rng, shaped, noise_floor)
}

fn bin_velocity(k: usize, m: usize, wavelength_m: f64, prt_s: f64) -> f64 {
    let half = m.div_ceil(2);
    let k_signed = if k < half {
        k as i64
    } else {
        k as i64 - m as i64
    };
    let f_k = k_signed as f64 / (m as f64 * prt_s);
    f_k * wavelength_m / 2.0
}

/// Prueba 1 del oráculo: espectro circular, tono puro en bins de borde
/// (incluida la discontinuidad de Nyquist), sesgo < 0.2 m/s.
#[test]
fn pure_tone_recovered_across_nyquist_edge() {
    const EDGE_BINS: [usize; 6] = [0, 1, 31, 32, 33, 63];
    const N_TRIALS: usize = 300;
    const BIAS_TOLERANCE: f64 = 0.2;

    let mut rng = StdRng::seed_from_u64(20260907);
    for &bin_idx in &EDGE_BINS {
        let v_true = bin_velocity(bin_idx, M, WAVELENGTH_M, PRT_S);
        let mut sum_v = 0.0f64;
        let mut n_ok = 0usize;
        for _ in 0..N_TRIALS {
            let y = generate_pure_tone_cell(1.0, bin_idx, M, 0.01, &mut rng);
            let est = spectral_moments(&y, WAVELENGTH_M, PRT_S);
            if let Some(v) = est.velocity_mps {
                sum_v += v;
                n_ok += 1;
            }
        }
        let v_mean = sum_v / n_ok as f64;
        let bias = v_mean - v_true;
        assert!(
            bias.abs() < BIAS_TOLERANCE,
            "bin={bin_idx}: v_true={v_true:.3} v_hat medio={v_mean:.3} sesgo={bias:+.4}"
        );
    }
}

/// Prueba 2 del oráculo: un solo modo, sesgo de V comparable al pulse-pair
/// en todo el barrido de σv (la varianza NO se exige mejor -- hallazgo
/// documentado del oráculo).
#[test]
fn single_mode_velocity_bias_within_tolerance() {
    const SIGMA_V_GRID: [f64; 4] = [0.5, 1.5, 3.0, 6.0];
    const NOISE_FLOOR: f64 = 0.05;
    const POWER_S: f64 = 1.0;
    const N_TRIALS: usize = 700;
    const V_BIAS_TOLERANCE: f64 = 0.5;

    let mut rng = StdRng::seed_from_u64(20260907);
    for &sigma_v in &SIGMA_V_GRID {
        let params = CellParams {
            power_s: POWER_S,
            mean_v: MEAN_V,
            sigma_v,
            wavelength_m: WAVELENGTH_M,
            prt_s: PRT_S,
            m: M,
            noise_floor: NOISE_FLOOR,
        };
        let mut sum_v = 0.0f64;
        let mut n_ok = 0usize;
        for _ in 0..N_TRIALS {
            let y = generate_cell(&params, &mut rng);
            if let Some(v) = spectral_moments(&y, WAVELENGTH_M, PRT_S).velocity_mps {
                sum_v += v;
                n_ok += 1;
            }
        }
        let bias = sum_v / n_ok as f64 - MEAN_V;
        assert!(
            bias.abs() < V_BIAS_TOLERANCE,
            "sv={sigma_v}: sesgo de V={bias:+.4} excede tolerancia {V_BIAS_TOLERANCE}"
        );
    }
}

/// Prueba 3 del oráculo: bimodal, el estimador espectral aísla el modo
/// dominante en >= 90% de las realizaciones.
#[test]
fn bimodal_recovers_dominant_mode() {
    const V_DOMINANT: f64 = 5.0;
    const V_SECONDARY: f64 = -15.0;
    const POWER_DOMINANT: f64 = 1.0;
    const POWER_SECONDARY: f64 = 0.1;
    const NOISE_FLOOR: f64 = 0.05;
    const N_TRIALS: usize = 800;
    const MIN_DOMINANT_RECOVERY_FRAC: f64 = 0.9;

    let mut rng = StdRng::seed_from_u64(20260907);
    let mut n_near_dominant = 0usize;
    for _ in 0..N_TRIALS {
        let y = generate_bimodal_cell(
            POWER_DOMINANT,
            V_DOMINANT,
            1.0,
            POWER_SECONDARY,
            V_SECONDARY,
            1.0,
            WAVELENGTH_M,
            PRT_S,
            M,
            NOISE_FLOOR,
            &mut rng,
        );
        if let Some(v) = spectral_moments(&y, WAVELENGTH_M, PRT_S).velocity_mps {
            if (v - V_DOMINANT).abs() < 3.0 {
                n_near_dominant += 1;
            }
        }
    }
    let frac = n_near_dominant as f64 / N_TRIALS as f64;
    assert!(
        frac >= MIN_DOMINANT_RECOVERY_FRAC,
        "fracción cerca del modo dominante={frac:.3} por debajo de {MIN_DOMINANT_RECOVERY_FRAC}"
    );
}
