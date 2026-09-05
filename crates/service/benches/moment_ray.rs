//! Benchmark de throughput del hot path real: `build_moment_ray` sobre un
//! radial de "peor caso" representativo (dual-pol, todos los momentos
//! activos, filtro de clutter GMAP + RFI encendidos) a dos tamaños de rango
//! — 100 celdas de referencia y 1840 celdas (`docs/dsp-plan.md` §3.1: alcance
//! máximo de reflectividad, 460 km a 250 m de espaciado).
//!
//! Esto es la semilla de la puerta de regresión de rendimiento que pide
//! `docs/dsp-plan.md` §10 ("Benchmark-regression gates (criterion) en los
//! hot loops... el binding constraint es compute"), no un resultado de
//! aceptación: ningún documento de este repositorio fija un presupuesto de
//! tiempo por radial (PRF máxima × celdas máximas no está cerrado, ver
//! `docs/dsp-plan.md` §11 "Sustained moment-estimation throughput"), así que
//! esto sirve para detectar regresiones relativas entre ejecuciones, no para
//! certificar un número absoluto contra un SLA que este repositorio no
//! declara todavía.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use lamula_contract::dsp_rcp::{clutter_filter, dealias_mode, estimator, moment_kind, Config};
use lamula_dsp_service::ray::build_moment_ray;
use lamula_ingest::AssembledRadial;
use lamula_simulator::{generate_dual_pol_cell, CellParams, DualPolParams};
use rand::rngs::StdRng;
use rand::SeedableRng;

const ALL_MOMENTS_MASK: u32 = (1 << moment_kind::UZ)
    | (1 << moment_kind::CZ)
    | (1 << moment_kind::V)
    | (1 << moment_kind::SQI)
    | (1 << moment_kind::SIG)
    | (1 << moment_kind::ZDR)
    | (1 << moment_kind::RHOHV)
    | (1 << moment_kind::PHIDP)
    | (1 << moment_kind::KDP);

fn worst_case_radial(n_gates: usize, n_pulses: usize) -> AssembledRadial {
    let cell = CellParams {
        power_s: 1.0,
        mean_v: 8.0,
        sigma_v: 1.5,
        wavelength_m: 0.10,
        prt_s: 1.0e-3,
        m: n_pulses,
        noise_floor: 0.02,
    };
    let dual = DualPolParams {
        zdr_db: 1.5,
        rho_hv: 0.97,
        phidp_deg: 15.0,
    };
    let mut rng = StdRng::seed_from_u64(20260905);
    let mut h_channel = Vec::with_capacity(n_gates);
    let mut v_channel = Vec::with_capacity(n_gates);
    for _ in 0..n_gates {
        let (h, v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
        h_channel.push(h);
        v_channel.push(v);
    }
    AssembledRadial {
        seq_start: 1,
        timestamp_ns_start: 0,
        trigger_count_start: 0,
        azimuth_raw: 0,
        elevation_raw: 0,
        prf_div: 1,
        pulse_width_idx: 0,
        pulse_mode: 0,
        cell_mode: 0,
        channel_mask: 0b0011,
        channels: vec![h_channel, v_channel],
        ray_flags: vec![0; n_pulses],
        dropped_pulses: 0,
    }
}

fn worst_case_config(n_gates: u16, n_pulses: u16) -> Config {
    Config {
        seq: 1,
        moment_mask: ALL_MOMENTS_MASK,
        n_pulses,
        n_gates,
        clutter_filter: clutter_filter::GMAP,
        dealias_mode: dealias_mode::NONE,
        sweep_mode: 0,
        estimator: estimator::PULSE_PAIR,
        rfi_filter: 1,
        range_dealias: 0,
        prf_ratio_num: 0,
        prf_ratio_den: 0,
        start_range_m: 0.0,
        gate_spacing_m: 250.0,
        prf_hz: 1200.0,
        sqi_threshold: 0.4,
        sig_threshold: 3.0,
        ccor_threshold: 20.0,
        log_threshold: -100.0,
        clutter_width_ms: 1.0,
        radar_constant_db: 65.0,
        noise_floor_dbm: -108.0,
        receiver_gain_db: 40.0,
        zdr_offset_db: 0.0,
        phidp_offset_deg: 0.0,
        antenna_isolation_db: 25.0,
        wavelength_m: 0.10,
        polarization_mode: 0,
        pad0: 0,
        burst_window_bins: 0,
    }
}

fn bench_moment_ray(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_moment_ray");
    for &n_gates in &[100usize, 1840usize] {
        let n_pulses = 64usize;
        let radial = worst_case_radial(n_gates, n_pulses);
        let config = worst_case_config(n_gates as u16, n_pulses as u16);
        group.bench_with_input(BenchmarkId::from_parameter(n_gates), &n_gates, |b, _| {
            b.iter(|| {
                build_moment_ray(
                    black_box(&radial),
                    black_box(&config),
                    1,
                    false,
                    1_000_000,
                    0.0,
                    None,
                )
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_moment_ray);
criterion_main!(benches);
