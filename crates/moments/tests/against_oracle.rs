//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/pulse_pair_moments.ipynb`.
//! Reproduce sus tolerancias exactas sobre una malla reducida de (SNR, σv, M)
//! -- la del oráculo es 4x4x4x400 realizaciones, aquí se recorta el número de
//! puntos de malla para mantener el test rápido sin cambiar tolerancia ni
//! N_TRIALS por punto.

use std::f64::consts::PI;

use lamula_moments::{pulse_pair_moments, PulsePairFlag};
use lamula_simulator::{gaussian_doppler_spectrum, generate_cell, CellParams};
use rand::rngs::StdRng;
use rand::SeedableRng;

const WAVELENGTH_M: f64 = 0.10;
const PRT_S: f64 = 1.0e-3;
const MEAN_V: f64 = 5.0;
const NOISE_FLOOR: f64 = 0.01;
const V_A: f64 = WAVELENGTH_M / (4.0 * PRT_S);

const Z_BIAS_TOLERANCE_DB: f64 = 1.0;
const V_BIAS_ABS_TOLERANCE: f64 = 0.5;
const W_BIAS_ABS_TOLERANCE: f64 = 3.0;

/// Verdad-terreno discreta del espectro que realmente genera `generate_cell`
/// (celda "Verdad-terreno EXACTA" del oráculo): a M pequeño y σv estrecho
/// frente al espaciado entre bins, la gaussiana continua se discretiza y su
/// centroide/ancho reales se apartan del valor continuo -- no es un error del
/// estimador, es la rejilla siendo demasiado gruesa.
fn discrete_truth(mean_v: f64, sigma_v: f64, m: usize) -> (f64, f64) {
    let spectrum = gaussian_doppler_spectrum(1.0, mean_v, sigma_v, WAVELENGTH_M, PRT_S, m);
    let half = m.div_ceil(2);
    let v_k: Vec<f64> = (0..m)
        .map(|k| {
            let k_signed = if k < half {
                k as i64
            } else {
                k as i64 - m as i64
            };
            let f = k_signed as f64 / (m as f64 * PRT_S);
            f * WAVELENGTH_M / 2.0
        })
        .collect();
    let centroid: f64 = v_k.iter().zip(&spectrum).map(|(v, p)| v * p).sum();
    let width: f64 = v_k
        .iter()
        .zip(&spectrum)
        .map(|(v, p)| p * (v - centroid).powi(2))
        .sum::<f64>()
        .sqrt();
    (centroid, width)
}

/// Media y error estándar circulares (von Mises) -- celda
/// `circular_mean_stderr` del oráculo. La velocidad vive en un círculo de
/// circunferencia `2*v_a`.
fn circular_mean(v_ests: &[f64]) -> f64 {
    let n = v_ests.len() as f64;
    let theta: Vec<f64> = v_ests.iter().map(|v| PI * v / V_A).collect();
    let c: f64 = theta.iter().map(|t| t.cos()).sum::<f64>() / n;
    let s: f64 = theta.iter().map(|t| t.sin()).sum::<f64>() / n;
    s.atan2(c) * V_A / PI
}

fn params(power_s: f64, sigma_v: f64, m: usize) -> CellParams {
    CellParams {
        power_s,
        mean_v: MEAN_V,
        sigma_v,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m,
        noise_floor: NOISE_FLOOR,
    }
}

/// Celda "Barrido (SNR, σv, M)" del oráculo: sesgo de Z, V y W dentro de
/// tolerancia en toda la malla (excepto SNR=0 dB, excluido igual que en el
/// oráculo).
#[test]
fn bias_within_tolerance_across_grid() {
    const N_TRIALS: usize = 400;
    let mut rng = StdRng::seed_from_u64(20260905);

    for &snr_db in &[10.0, 20.0, 30.0] {
        let power_s = NOISE_FLOOR * 10f64.powf(snr_db / 10.0);
        for &sigma_v in &[0.5, 1.5, 3.0, 6.0] {
            for &m in &[16, 32, 64, 128] {
                let (v_true, w_true) = discrete_truth(MEAN_V, sigma_v, m);
                let p = params(power_s, sigma_v, m);

                let mut z_sum = 0.0;
                let mut v_ests = Vec::with_capacity(N_TRIALS);
                let mut w_ok = Vec::new();
                for _ in 0..N_TRIALS {
                    let y = generate_cell(&p, &mut rng);
                    let est = pulse_pair_moments(&y, WAVELENGTH_M, PRT_S);
                    z_sum += est.s_linear;
                    v_ests.push(est.velocity_mps);
                    if est.flag == PulsePairFlag::Ok {
                        w_ok.push(est.spectrum_width_mps.unwrap());
                    }
                }

                let z_bias_db = 10.0 * ((z_sum / N_TRIALS as f64) / power_s).log10();
                assert!(
                    z_bias_db.abs() < Z_BIAS_TOLERANCE_DB,
                    "SNR={snr_db} sv={sigma_v} M={m}: sesgo Z={z_bias_db:.3} dB excede {Z_BIAS_TOLERANCE_DB} dB"
                );

                let v_bias = circular_mean(&v_ests) - v_true;
                assert!(
                    v_bias.abs() < V_BIAS_ABS_TOLERANCE,
                    "SNR={snr_db} sv={sigma_v} M={m}: sesgo V={v_bias:.3} m/s excede {V_BIAS_ABS_TOLERANCE} m/s"
                );

                if w_ok.len() > 5 {
                    let w_mean = w_ok.iter().sum::<f64>() / w_ok.len() as f64;
                    let w_bias = w_mean - w_true;
                    assert!(
                        w_bias.abs() < W_BIAS_ABS_TOLERANCE,
                        "SNR={snr_db} sv={sigma_v} M={m}: sesgo W={w_bias:.3} m/s excede {W_BIAS_ABS_TOLERANCE} m/s (tolerancia amplia, ver nota del oráculo)"
                    );
                }
            }
        }
    }
}

/// Ningún caso marcado `Ok` produce NaN en el ancho espectral, incluyendo los
/// dos extremos que la página señala como delicados (σv pequeño, σv grande
/// con M pequeño).
#[test]
fn ok_estimates_never_nan() {
    const N_TRIALS: usize = 400;
    let mut rng = StdRng::seed_from_u64(20260905);

    for &sigma_v in &[0.5, 6.0] {
        let p = params(NOISE_FLOOR * 100.0, sigma_v, 16);
        for _ in 0..N_TRIALS {
            let y = generate_cell(&p, &mut rng);
            let est = pulse_pair_moments(&y, WAVELENGTH_M, PRT_S);
            assert!(!est.velocity_mps.is_nan(), "velocidad NaN con σv={sigma_v}");
            if est.flag == PulsePairFlag::Ok {
                assert!(
                    !est.spectrum_width_mps.unwrap().is_nan(),
                    "ancho NaN marcado Ok con σv={sigma_v}"
                );
            }
        }
    }
}

/// σv=0.5, M=16 debe mostrar saturación apreciable (>5%) -- extremo "σv muy
/// pequeño" declarado por la página, comportamiento visible en vez de oculto.
#[test]
fn narrow_sigma_v_shows_appreciable_saturation() {
    const N_TRIALS: usize = 400;
    let mut rng = StdRng::seed_from_u64(20260905);
    let p = params(NOISE_FLOOR * 100.0, 0.5, 16);

    let sat_count = (0..N_TRIALS)
        .filter(|_| {
            let y = generate_cell(&p, &mut rng);
            pulse_pair_moments(&y, WAVELENGTH_M, PRT_S).flag == PulsePairFlag::Saturated
        })
        .count();

    let sat_frac = sat_count as f64 / N_TRIALS as f64;
    assert!(
        sat_frac > 0.05,
        "saturación en σv=0.5, M=16 fue {:.1}%, se esperaba >5%",
        sat_frac * 100.0
    );
}

/// Celda "Escalado empírico de la varianza de V con M": la dispersión de la
/// velocidad estimada decrece de forma monótona al crecer M.
#[test]
fn velocity_std_decreases_monotonically_with_m() {
    const N_TRIALS: usize = 800;
    let mut rng = StdRng::seed_from_u64(20260905);
    let power_s = NOISE_FLOOR * 100.0;

    let mut stds = Vec::new();
    for &m in &[16, 32, 64, 128, 256] {
        let p = params(power_s, 3.0, m);
        let vs: Vec<f64> = (0..N_TRIALS)
            .map(|_| {
                let y = generate_cell(&p, &mut rng);
                pulse_pair_moments(&y, WAVELENGTH_M, PRT_S).velocity_mps
            })
            .collect();
        let mean = vs.iter().sum::<f64>() / N_TRIALS as f64;
        let var = vs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (N_TRIALS - 1) as f64;
        stds.push(var.sqrt());
    }

    for w in stds.windows(2) {
        assert!(
            w[0] > w[1],
            "std(V) no decrece monótonamente con M: {stds:?}"
        );
    }
}
