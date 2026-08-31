//! Tests de aceptación del simulador (ver `docs/algorithms/simulador-iq.md`
//! §"Criterio de aceptación"): el simulador se valida contra sí mismo por vía
//! estadística, no contra el resto del DSP.

use std::f64::consts::PI;

use lamula_contract::drx_dsp::{HEADER_SIZE, MAGIC, RAY_SIZE};
use lamula_simulator::{
    gaussian_doppler_spectrum, generate_cell, generate_dual_pol_cell, pack_rays, CellParams,
    DualPolParams, RayHeaderFields,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rustfft::num_complex::Complex64;

fn test_params(m: usize) -> CellParams {
    CellParams {
        power_s: 1.0,
        mean_v: 5.0,
        sigma_v: 1.5,
        wavelength_m: 0.10,
        prt_s: 1.0e-3,
        m,
        noise_floor: 0.0,
    }
}

/// Autocovarianza analítica de la serie generada, calculada independientemente
/// del código de `generate_cell` (suma directa sobre el espectro discreto),
/// tal como exige el método del roadmap: el oráculo no comparte camino con la
/// implementación que verifica.
fn analytic_acf(spectrum: &[f64], lag: usize) -> Complex64 {
    let m = spectrum.len();
    let mut acc = Complex64::new(0.0, 0.0);
    for (k, &s) in spectrum.iter().enumerate() {
        let phase = -2.0 * PI * (k as f64) * (lag as f64) / (m as f64);
        acc += Complex64::new(s, 0.0) * Complex64::new(phase.cos(), phase.sin());
    }
    acc
}

#[test]
fn autocovariance_matches_analytic_model() {
    let params = test_params(64);
    let spectrum = gaussian_doppler_spectrum(
        params.power_s,
        params.mean_v,
        params.sigma_v,
        params.wavelength_m,
        params.prt_s,
        params.m,
    );

    const N: usize = 20_000;
    let mut rng = StdRng::seed_from_u64(42);

    let mut lag0_samples = Vec::with_capacity(N);
    let mut lag1_samples = Vec::with_capacity(N);
    for _ in 0..N {
        let x = generate_cell(&params, &mut rng);
        lag0_samples.push(x[0] * x[0].conj());
        lag1_samples.push(x[0] * x[1].conj());
    }

    for (lag, samples) in [(0usize, &lag0_samples), (1usize, &lag1_samples)] {
        let mean = samples.iter().fold(Complex64::new(0.0, 0.0), |a, b| a + b) / N as f64;
        let variance =
            samples.iter().map(|x| (x - mean).norm_sqr()).sum::<f64>() / (N as f64 - 1.0);
        let stderr = (variance / N as f64).sqrt();

        let analytic = analytic_acf(&spectrum, lag);
        let diff = (mean - analytic).norm();

        // Tolerancia derivada del propio muestreo (error estándar empírico),
        // no un valor fijo arbitrario — margen de 5 sigma para una tasa de
        // falso-fallo despreciable en CI.
        assert!(
            diff < 5.0 * stderr,
            "lag {lag}: |media muestral - analítico| = {diff:e} excede 5*stderr = {:e} (media={mean:?}, analítico={analytic:?})",
            5.0 * stderr
        );
    }
}

#[test]
fn power_distribution_is_exponential() {
    // La potencia instantánea de un proceso gaussiano complejo es marginal-
    // mente exponencial (amplitud Rayleigh) sea cual sea su correlación
    // temporal, así que basta agrupar todas las muestras de muchas
    // realizaciones cortas.
    let params = test_params(16);
    let mut rng = StdRng::seed_from_u64(7);

    const REALIZATIONS: usize = 4000;
    let mut powers = Vec::with_capacity(REALIZATIONS * params.m);
    for _ in 0..REALIZATIONS {
        let x = generate_cell(&params, &mut rng);
        powers.extend(x.iter().map(|c| c.norm_sqr()));
    }

    // Bondad de ajuste chi-cuadrado contra la exponencial de media power_s,
    // en 8 bins de igual probabilidad (cuantiles teóricos), con un umbral muy
    // laxo (p ~ 1e-4, 7 grados de libertad) para no ser un test frágil.
    const BINS: usize = 8;
    let mean = params.power_s;
    let mut counts = [0usize; BINS];
    for &p in &powers {
        // CDF exponencial: F(p) = 1 - exp(-p/mean). Cuantil -> bin uniforme.
        let cdf = 1.0 - (-p / mean).exp();
        let bin = ((cdf * BINS as f64) as usize).min(BINS - 1);
        counts[bin] += 1;
    }
    let expected = powers.len() as f64 / BINS as f64;
    let chi_square: f64 = counts
        .iter()
        .map(|&c| (c as f64 - expected).powi(2) / expected)
        .sum();

    // Chi-cuadrado crítico para 7 g.l. a p=1e-4 es ~29.9 (tablas estándar).
    assert!(
        chi_square < 29.9,
        "chi-cuadrado = {chi_square} excede el crítico (7 g.l., p=1e-4); distribución de potencia no es exponencial"
    );
}

#[test]
fn phase_is_uniform() {
    let params = test_params(16);
    let mut rng = StdRng::seed_from_u64(99);

    const REALIZATIONS: usize = 4000;
    let mut phases = Vec::with_capacity(REALIZATIONS * params.m);
    for _ in 0..REALIZATIONS {
        let x = generate_cell(&params, &mut rng);
        phases.extend(x.iter().map(|c| c.arg()));
    }

    const BINS: usize = 8;
    let mut counts = [0usize; BINS];
    for &phase in &phases {
        let normalized = (phase + PI) / (2.0 * PI); // -> [0,1)
        let bin = ((normalized * BINS as f64) as usize).min(BINS - 1);
        counts[bin] += 1;
    }
    let expected = phases.len() as f64 / BINS as f64;
    let chi_square: f64 = counts
        .iter()
        .map(|&c| (c as f64 - expected).powi(2) / expected)
        .sum();

    // Mismo umbral laxo que el test de potencia (7 g.l., p=1e-4).
    assert!(
        chi_square < 29.9,
        "chi-cuadrado = {chi_square} excede el crítico; la fase no es uniforme"
    );
}

#[test]
fn packed_ray_round_trips_header_fields() {
    let params = test_params(8);
    let mut rng = StdRng::seed_from_u64(1);

    const BINS: usize = 3;
    let cells: Vec<Vec<Complex64>> = (0..BINS)
        .map(|_| generate_cell(&params, &mut rng))
        .collect();

    let fields = RayHeaderFields {
        seq_start: 100,
        timestamp_ns_start: 1_000_000,
        timestamp_step_ns: params.prt_s as u64 * 1_000_000_000,
        trigger_count_start: 100,
        azimuth_raw: 12345,
        elevation_raw: 678,
        prf_div: 4,
        pulse_width_idx: 2,
        pulse_mode: 0,
        cell_mode: 0,
        channel_mask: 0b0001,
        ray_flags: 0,
    };

    let frames = pack_rays(&fields, &[cells], i16::MAX);
    assert_eq!(frames.len(), params.m);

    let payload_len = BINS * 2 * std::mem::size_of::<i16>();
    for (i, frame) in frames.iter().enumerate() {
        assert_eq!(frame.len(), HEADER_SIZE + RAY_SIZE + payload_len);

        let magic = u32::from_le_bytes(frame[0..4].try_into().unwrap());
        assert_eq!(magic, MAGIC);

        let msg_type = frame[6];
        assert_eq!(msg_type, 1, "MsgType::Ray");

        let declared_payload_len = u32::from_le_bytes(frame[8..12].try_into().unwrap());
        assert_eq!(declared_payload_len as usize, payload_len);

        let seq = u32::from_le_bytes(frame[12..16].try_into().unwrap());
        assert_eq!(seq, fields.seq_start.wrapping_add(i as u32));

        // Offsets dentro de `Ray` (empieza en HEADER_SIZE=12): seq(4) +
        // timestamp_ns(8) + trigger_count(4) + azimuth_raw(4) +
        // elevation_raw(4) + prf_div(4) = 28 bytes antes de `bins`.
        let bins_field = u16::from_le_bytes(frame[40..42].try_into().unwrap());
        assert_eq!(bins_field as usize, BINS);

        let n_channels = frame[45];
        assert_eq!(n_channels, 1);

        let channel_mask = frame[46];
        assert_eq!(channel_mask, fields.channel_mask);
    }
}

fn test_dual_pol_params() -> DualPolParams {
    DualPolParams {
        zdr_db: 2.0,
        rho_hv: 0.97,
        phidp_deg: 30.0,
    }
}

#[test]
fn cross_correlation_matches_rho_hv_and_phidp() {
    let params = test_params(16);
    let dual = test_dual_pol_params();
    let mut rng = StdRng::seed_from_u64(11);

    const N: usize = 20_000;
    let mut cross_samples = Vec::with_capacity(N);
    for _ in 0..N {
        let (h, v) = generate_dual_pol_cell(&params, &dual, &mut rng);
        cross_samples.push(h[0] * v[0].conj());
    }

    let power_v = params.power_s / 10f64.powf(dual.zdr_db / 10.0);
    let norm = (params.power_s * power_v).sqrt();

    let mean = cross_samples
        .iter()
        .fold(Complex64::new(0.0, 0.0), |a, b| a + b)
        / N as f64;
    let variance = cross_samples
        .iter()
        .map(|x| (x - mean).norm_sqr())
        .sum::<f64>()
        / (N as f64 - 1.0);
    let stderr = (variance / N as f64).sqrt();

    let rho_hat = mean / norm;
    let expected = Complex64::from_polar(dual.rho_hv, dual.phidp_deg.to_radians());
    let diff = (mean - expected * norm).norm();

    assert!(
        diff < 5.0 * stderr,
        "|media muestral - esperado| = {diff:e} excede 5*stderr = {:e} (rho_hat={rho_hat:?}, esperado_modulo={}, esperado_fase_deg={})",
        5.0 * stderr,
        dual.rho_hv,
        dual.phidp_deg
    );
}

#[test]
fn power_ratio_matches_zdr() {
    let params = test_params(16);
    let dual = test_dual_pol_params();
    let mut rng = StdRng::seed_from_u64(23);

    const N: usize = 20_000;
    let mut power_h_total = 0.0;
    let mut power_v_total = 0.0;
    for _ in 0..N {
        let (h, v) = generate_dual_pol_cell(&params, &dual, &mut rng);
        power_h_total += h.iter().map(|c| c.norm_sqr()).sum::<f64>();
        power_v_total += v.iter().map(|c| c.norm_sqr()).sum::<f64>();
    }
    let ratio_hat = power_v_total / power_h_total;
    let expected_ratio = 10f64.powf(-dual.zdr_db / 10.0);

    // Tolerancia laxa (2%) — potencia total sobre N*M muestras converge
    // rápido, no hace falta el aparato del error estándar de la fase/módulo.
    assert!(
        (ratio_hat - expected_ratio).abs() / expected_ratio < 0.02,
        "razón de potencia V/H = {ratio_hat}, esperada {expected_ratio} (ZDR={} dB)",
        dual.zdr_db
    );
}

#[test]
fn packed_ray_two_channels_interleaves_channel_fastest() {
    let params = test_params(4);
    let dual = test_dual_pol_params();
    let mut rng = StdRng::seed_from_u64(5);

    const BINS: usize = 2;
    let mut h_cells = Vec::with_capacity(BINS);
    let mut v_cells = Vec::with_capacity(BINS);
    for _ in 0..BINS {
        let (h, v) = generate_dual_pol_cell(&params, &dual, &mut rng);
        h_cells.push(h);
        v_cells.push(v);
    }

    let fields = RayHeaderFields {
        seq_start: 0,
        timestamp_ns_start: 0,
        timestamp_step_ns: 1,
        trigger_count_start: 0,
        azimuth_raw: 0,
        elevation_raw: 0,
        prf_div: 4,
        pulse_width_idx: 0,
        pulse_mode: 0,
        cell_mode: 0,
        channel_mask: 0b0011,
        ray_flags: 0,
    };

    let frames = pack_rays(&fields, &[h_cells.clone(), v_cells.clone()], i16::MAX);
    assert_eq!(frames.len(), params.m);

    let payload_len = BINS * 2 /* canales */ * 2 /* I,Q */ * std::mem::size_of::<i16>();
    for (i, frame) in frames.iter().enumerate() {
        assert_eq!(frame.len(), HEADER_SIZE + RAY_SIZE + payload_len);
        assert_eq!(frame[45], 2, "n_channels");

        let payload = &frame[HEADER_SIZE + RAY_SIZE..];
        for bin in 0..BINS {
            let base = bin * 2 * 4; // 2 canales * (I:i16 + Q:i16) = 8 bytes/bin
            let h_i = i16::from_le_bytes(payload[base..base + 2].try_into().unwrap());
            let h_q = i16::from_le_bytes(payload[base + 2..base + 4].try_into().unwrap());
            let v_i = i16::from_le_bytes(payload[base + 4..base + 6].try_into().unwrap());
            let v_q = i16::from_le_bytes(payload[base + 6..base + 8].try_into().unwrap());

            let expected_h = h_cells[bin][i];
            let expected_v = v_cells[bin][i];
            assert_eq!(h_i, (expected_h.re * i16::MAX as f64).round() as i16);
            assert_eq!(h_q, (expected_h.im * i16::MAX as f64).round() as i16);
            assert_eq!(v_i, (expected_v.re * i16::MAX as f64).round() as i16);
            assert_eq!(v_q, (expected_v.im * i16::MAX as f64).round() as i16);
        }
    }
}
