//! Paso 3 del método (`docs/algorithms/roadmap.md` §"Método de estudio"):
//! test de contraste numérico contra `tools/oracles/burst_fase_afc.ipynb`.
//! Reproduce sus escenarios y tolerancias exactas (celdas 8 y 10). Las
//! celdas 4 y 6 (fase y frecuencia del burst) están en los tests unitarios
//! de `phase.rs`.

use lamula_burst::{burst_phase_estimate, correct_phase, loop_gain, AfcLoop};
use lamula_noise::{noise_floor_estimate, subtract_noise, total_power};
use lamula_simulator::{generate_cell, CellParams};
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;
use std::f64::consts::PI;

const AMP_BURST: f64 = 10.0;
const BURST_NOISE_VAR: f64 = 0.01;
const N_BURST: usize = 32;
const DT_FAST_S: f64 = 100e-9;

fn complex_gaussian(rng: &mut StdRng, variance: f64) -> Complex64 {
    let sigma = (variance / 2.0).sqrt();
    let re: f64 = StandardNormal.sample(rng);
    let im: f64 = StandardNormal.sample(rng);
    Complex64::new(re * sigma, im * sigma)
}

fn simulated_burst_for_phase(rng: &mut StdRng, phi_true: f64) -> Vec<Complex64> {
    (0..N_BURST)
        .map(|_| {
            Complex64::from_polar(AMP_BURST, phi_true) + complex_gaussian(rng, BURST_NOISE_VAR)
        })
        .collect()
}

fn simulated_burst_for_freq(rng: &mut StdRng, phi0: f64, f_true: f64, amp: f64) -> Vec<Complex64> {
    (0..N_BURST)
        .map(|i| {
            let phase = phi0 + 2.0 * PI * f_true * i as f64 * DT_FAST_S;
            Complex64::from_polar(amp, phase) + complex_gaussian(rng, BURST_NOISE_VAR)
        })
        .collect()
}

/// Igual que `pulse_pair_velocity` en el oráculo: herramienta para demostrar
/// la corrección, no el objeto de estudio de esta página — su propio oráculo
/// está en `docs/algorithms/pulse-pair-moments.md`.
fn pulse_pair_velocity(y: &[Complex64], wavelength_m: f64, prt_s: f64) -> f64 {
    let mut r1 = Complex64::new(0.0, 0.0);
    for w in y.windows(2) {
        r1 += w[0] * w[1].conj();
    }
    r1 /= (y.len() - 1) as f64;
    -wavelength_m / (4.0 * PI * prt_s) * r1.arg()
}

/// Celda 8 del oráculo: momentos de un escenario de magnetrón corregidos por
/// fase deben coincidir, dentro de 5 errores estándar, con los del mismo
/// escenario generado con transmisor coherente; sin corregir, la velocidad no
/// recupera la verdad-terreno.
#[test]
fn phase_correction_erases_magnetron_vs_coherent_difference() {
    const POWER_S: f64 = 1.0;
    const NOISE_FLOOR: f64 = 0.01;
    const MEAN_V: f64 = 5.0;
    const SIGMA_V: f64 = 1.5;
    const WAVELENGTH_M: f64 = 0.10;
    const PRT_S: f64 = 1.0e-3;
    const M: usize = 64;
    const N_TRIALS: usize = 1500;

    let params_signal_only = CellParams {
        power_s: POWER_S,
        mean_v: MEAN_V,
        sigma_v: SIGMA_V,
        wavelength_m: WAVELENGTH_M,
        prt_s: PRT_S,
        m: M,
        noise_floor: 0.0,
    };

    let mut rng = StdRng::seed_from_u64(20260904);
    let mut z_coh = Vec::with_capacity(N_TRIALS);
    let mut v_coh = Vec::with_capacity(N_TRIALS);
    let mut z_corr = Vec::with_capacity(N_TRIALS);
    let mut v_corr = Vec::with_capacity(N_TRIALS);
    let mut v_raw = Vec::with_capacity(N_TRIALS);

    let moments_of = |y: &[Complex64]| -> (f64, f64) {
        let r0_hat = total_power(y);
        let n_hat = noise_floor_estimate(y);
        let z_hat = subtract_noise(r0_hat, n_hat).unwrap_or(0.0);
        let v_hat = pulse_pair_velocity(y, WAVELENGTH_M, PRT_S);
        (z_hat, v_hat)
    };

    for _ in 0..N_TRIALS {
        let x = generate_cell(&params_signal_only, &mut rng);

        let y_coh: Vec<Complex64> = x
            .iter()
            .map(|&s| s + complex_gaussian(&mut rng, NOISE_FLOOR))
            .collect();

        let phi_true: Vec<f64> = (0..M).map(|_| rng.gen_range(-PI..PI)).collect();
        let y_mag_raw: Vec<Complex64> = x
            .iter()
            .zip(&phi_true)
            .map(|(&s, &phi)| {
                s * Complex64::from_polar(1.0, phi) + complex_gaussian(&mut rng, NOISE_FLOOR)
            })
            .collect();

        let phi_hat: Vec<f64> = phi_true
            .iter()
            .map(|&phi| burst_phase_estimate(&simulated_burst_for_phase(&mut rng, phi)))
            .collect();
        let y_mag_corr: Vec<Complex64> = y_mag_raw
            .iter()
            .zip(&phi_hat)
            .map(|(&s, &phi)| correct_phase(s, phi))
            .collect();

        let (zc, vc) = moments_of(&y_coh);
        let (zr, vr) = moments_of(&y_mag_corr);
        z_coh.push(zc);
        v_coh.push(vc);
        z_corr.push(zr);
        v_corr.push(vr);
        v_raw.push(pulse_pair_velocity(&y_mag_raw, WAVELENGTH_M, PRT_S));
    }

    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let var =
        |v: &[f64], m: f64| v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64;

    let z_coh_mean = mean(&z_coh);
    let v_coh_mean = mean(&v_coh);
    let z_corr_mean = mean(&z_corr);
    let v_corr_mean = mean(&v_corr);
    let v_raw_mean = mean(&v_raw);

    let z_stderr = (var(&z_coh, z_coh_mean) / N_TRIALS as f64
        + var(&z_corr, z_corr_mean) / N_TRIALS as f64)
        .sqrt();
    let v_stderr = (var(&v_coh, v_coh_mean) / N_TRIALS as f64
        + var(&v_corr, v_corr_mean) / N_TRIALS as f64)
        .sqrt();

    assert!(
        (z_coh_mean - z_corr_mean).abs() < 5.0 * z_stderr,
        "Z corregida {z_corr_mean:.4} no coincide con Z coherente {z_coh_mean:.4} dentro de 5 stderr ({z_stderr:.4})"
    );
    assert!(
        (v_coh_mean - v_corr_mean).abs() < 5.0 * v_stderr,
        "V corregida {v_corr_mean:.3} no coincide con V coherente {v_coh_mean:.3} dentro de 5 stderr ({v_stderr:.3})"
    );
    assert!(
        (v_raw_mean - MEAN_V).abs() > 4.0,
        "V sin corregir {v_raw_mean:.3} recupera la verdad-terreno {MEAN_V} — la corrección de fase no está haciendo nada"
    );
}

/// Celda 10 del oráculo: comportamiento del lazo de AFC ante escalón, rampa,
/// ruido puro y pérdida de burst.
#[test]
fn afc_loop_matches_oracle_step_ramp_noise_and_loss_behavior() {
    const UPDATE_PERIOD_S: f64 = 0.02;
    const TAU_S: f64 = 2.0;
    const AMP_THRESHOLD: f64 = 2.0;
    const N_UPDATES: usize = 400;
    const F_OFFSET_HZ: f64 = 50_000.0;

    let gain = loop_gain(UPDATE_PERIOD_S, TAU_S);
    let mut rng = StdRng::seed_from_u64(20260904);

    // --- escalón ---
    let f_step: Vec<f64> = (0..N_UPDATES)
        .map(|k| if k < 50 { 0.0 } else { F_OFFSET_HZ })
        .collect();
    let mut loop_step = AfcLoop::new(gain, AMP_THRESHOLD, DT_FAST_S);
    let freq_step: Vec<f64> = f_step
        .iter()
        .map(|&f| {
            let phi0 = rng_phi0(&mut rng);
            let burst = simulated_burst_for_freq(&mut rng, phi0, f, AMP_BURST);
            loop_step.update(&burst).freq_hz
        })
        .collect();

    let t95_idx = freq_step[50..]
        .iter()
        .position(|&f| f > 0.95 * F_OFFSET_HZ)
        .map(|i| i + 50)
        .expect("el lazo no converge al escalón");
    let t95_s = (t95_idx - 50) as f64 * UPDATE_PERIOD_S;
    let overshoot_hz = freq_step.iter().cloned().fold(f64::MIN, f64::max) - F_OFFSET_HZ;

    assert!(
        t95_s < 4.0 * TAU_S,
        "convergencia al escalón {t95_s:.2}s excede 4tau ({:.0}s)",
        4.0 * TAU_S
    );
    assert!(
        overshoot_hz < 50.0,
        "sobreoscilación {overshoot_hz:.1} Hz excede el margen (0 Hz, primer orden)"
    );

    // --- rampa ---
    const RAMP_RATE_HZ_S: f64 = 2000.0;
    let f_ramp: Vec<f64> = (0..N_UPDATES)
        .map(|k| RAMP_RATE_HZ_S * k as f64 * UPDATE_PERIOD_S)
        .collect();
    let mut loop_ramp = AfcLoop::new(gain, AMP_THRESHOLD, DT_FAST_S);
    let freq_ramp: Vec<f64> = f_ramp
        .iter()
        .map(|&f| {
            let phi0 = rng_phi0(&mut rng);
            let burst = simulated_burst_for_freq(&mut rng, phi0, f, AMP_BURST);
            loop_ramp.update(&burst).freq_hz
        })
        .collect();
    let steady_lag = (300..N_UPDATES)
        .map(|k| f_ramp[k] - freq_ramp[k])
        .sum::<f64>()
        / (N_UPDATES - 300) as f64;
    let expected_lag = RAMP_RATE_HZ_S * TAU_S;
    assert!(
        (steady_lag - expected_lag).abs() < 0.2 * expected_lag,
        "retardo de rampa en régimen {steady_lag:.1} Hz fuera de 20% de {expected_lag:.1} Hz"
    );

    // --- ruido puro (sin deriva) ---
    let mut loop_noise = AfcLoop::new(gain, AMP_THRESHOLD, DT_FAST_S);
    let freq_noise: Vec<f64> = (0..N_UPDATES)
        .map(|_| {
            let phi0 = rng_phi0(&mut rng);
            let burst = simulated_burst_for_freq(&mut rng, phi0, 0.0, AMP_BURST);
            loop_noise.update(&burst).freq_hz
        })
        .collect();
    let raw_measurements: Vec<f64> = (0..1000)
        .map(|_| {
            let burst = simulated_burst_for_freq(&mut rng, 0.0, 0.0, AMP_BURST);
            lamula_burst::burst_freq_estimate(&burst, DT_FAST_S)
        })
        .collect();
    let raw_mean = mean_of(&raw_measurements);
    let raw_std = (raw_measurements
        .iter()
        .map(|v| (v - raw_mean).powi(2))
        .sum::<f64>()
        / raw_measurements.len() as f64)
        .sqrt();
    let loop_mean = mean_of(&freq_noise[200..]);
    let loop_std = (freq_noise[200..]
        .iter()
        .map(|v| (v - loop_mean).powi(2))
        .sum::<f64>()
        / freq_noise[200..].len() as f64)
        .sqrt();
    assert!(
        loop_std < 0.5 * raw_std,
        "std del lazo en régimen {loop_std:.1} Hz no filtra el ruido de medida cruda {raw_std:.1} Hz"
    );

    // --- pérdida de burst: congelamiento + BITE ---
    let mut loop_loss = AfcLoop::new(gain, AMP_THRESHOLD, DT_FAST_S);
    let mut freq_loss = Vec::with_capacity(N_UPDATES);
    let mut bite_loss = Vec::with_capacity(N_UPDATES);
    for k in 0..N_UPDATES {
        let amp = if (150..200).contains(&k) {
            0.1
        } else {
            AMP_BURST
        };
        let phi0 = rng_phi0(&mut rng);
        let burst = simulated_burst_for_freq(&mut rng, phi0, 20_000.0, amp);
        let update = loop_loss.update(&burst);
        freq_loss.push(update.freq_hz);
        bite_loss.push(update.bite);
    }
    let frozen = freq_loss[150..200].iter().all(|&f| f == freq_loss[149]);
    let bite_correct = bite_loss[150..200].iter().all(|&b| b)
        && !bite_loss[..150].iter().any(|&b| b)
        && !bite_loss[200..].iter().any(|&b| b);
    assert!(
        frozen,
        "el lazo no se congela en el último valor válido durante la pérdida de burst"
    );
    assert!(
        bite_correct,
        "BITE no se emite exactamente durante la ventana de pérdida de burst"
    );
}

fn rng_phi0(rng: &mut StdRng) -> f64 {
    rng.gen_range(-PI..PI)
}

fn mean_of(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}
