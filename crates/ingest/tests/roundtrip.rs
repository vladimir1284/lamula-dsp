//! Prueba de que `wire::decode_ray_frame` + `assembly::RadialAssembler` son
//! el inverso exacto (salvo error de cuantización int16) de
//! `lamula_simulator::pack_rays`.

use lamula_ingest::{decode_ray_frame, RadialAssembler};
use lamula_simulator::{generate_cell, pack_rays, CellParams, RayHeaderFields};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rustfft::num_complex::Complex64;

const FULL_SCALE: i16 = i16::MAX;

fn params(m: usize) -> CellParams {
    CellParams {
        // Pequeña a propósito: `quantize()` mapea amplitud 1.0 a
        // `full_scale_counts` y satura fuera de rango (ver
        // `crates/simulator/src/ray.rs::quantize`). Con `power_s = 1.0` la
        // envolvente Rayleigh de una señal compleja gaussiana excede
        // amplitud 1.0 más de un tercio de las veces — hay que dejar
        // suficiente margen para que la prueba compare cuantización, no
        // saturación.
        power_s: 0.01,
        mean_v: 5.0,
        sigma_v: 1.0,
        wavelength_m: 0.10,
        prt_s: 1.0e-3,
        m,
        noise_floor: 0.0,
    }
}

fn header_fields(seq_start: u32, prt_s: f64) -> RayHeaderFields {
    RayHeaderFields {
        seq_start,
        timestamp_ns_start: 0,
        timestamp_step_ns: (prt_s * 1.0e9) as u64,
        trigger_count_start: seq_start,
        azimuth_raw: 4096,
        elevation_raw: 512,
        prf_div: 4,
        pulse_width_idx: 1,
        pulse_mode: 0,
        cell_mode: 0,
        channel_mask: 0b0001,
        ray_flags: 0,
    }
}

#[test]
fn decode_and_assemble_recovers_original_samples_within_quantization() {
    const BINS: usize = 4;
    const M: usize = 32;
    let p = params(M);
    let mut rng = StdRng::seed_from_u64(7);
    let cells: Vec<Vec<Complex64>> = (0..BINS).map(|_| generate_cell(&p, &mut rng)).collect();

    let fields = header_fields(1000, p.prt_s);
    let wire_frames = pack_rays(&fields, std::slice::from_ref(&cells), FULL_SCALE);
    assert_eq!(wire_frames.len(), M);

    let mut assembler = RadialAssembler::new(M as u16);
    let mut assembled = None;
    for raw in &wire_frames {
        let frame = decode_ray_frame(raw, FULL_SCALE).expect("decodifica");
        assembled = assembler.feed(frame).expect("ensambla");
    }
    let radial = assembled.expect("radial completo tras M pulsos");

    assert_eq!(radial.seq_start, fields.seq_start);
    assert_eq!(radial.azimuth_raw, fields.azimuth_raw);
    assert_eq!(radial.elevation_raw, fields.elevation_raw);
    assert_eq!(radial.prf_div, fields.prf_div);
    assert_eq!(radial.dropped_pulses, 0);
    assert_eq!(radial.channels.len(), 1);
    assert_eq!(radial.channels[0].len(), BINS);

    let tol = 1.0 / FULL_SCALE as f64;
    for (bin, cell) in cells.iter().enumerate() {
        for (pulse, expected) in cell.iter().enumerate() {
            let got = radial.channels[0][bin][pulse];
            assert!(
                (expected.re - got.re).abs() <= tol && (expected.im - got.im).abs() <= tol,
                "bin {bin} pulso {pulse}: esperado {expected:?}, decodificado {got:?}"
            );
        }
    }
}

#[test]
fn two_channels_decode_without_swapping_iq_or_channel() {
    const BINS: usize = 2;
    const M: usize = 8;
    let p = params(M);
    let mut rng = StdRng::seed_from_u64(21);
    let ch0: Vec<Vec<Complex64>> = (0..BINS).map(|_| generate_cell(&p, &mut rng)).collect();
    let ch1: Vec<Vec<Complex64>> = (0..BINS).map(|_| generate_cell(&p, &mut rng)).collect();

    let mut fields = header_fields(0, p.prt_s);
    fields.channel_mask = 0b0011;
    let wire_frames = pack_rays(&fields, &[ch0.clone(), ch1.clone()], FULL_SCALE);

    let mut assembler = RadialAssembler::new(M as u16);
    let mut assembled = None;
    for raw in &wire_frames {
        let frame = decode_ray_frame(raw, FULL_SCALE).expect("decodifica");
        assembled = assembler.feed(frame).expect("ensambla");
    }
    let radial = assembled.expect("radial completo");

    assert_eq!(radial.channels.len(), 2);
    let tol = 1.0 / FULL_SCALE as f64;
    for (bin, (cell0, cell1)) in ch0.iter().zip(ch1.iter()).enumerate() {
        for (pulse, (&expected0, &expected1)) in cell0.iter().zip(cell1.iter()).enumerate() {
            assert!((radial.channels[0][bin][pulse] - expected0).norm() <= tol * 1.5);
            assert!((radial.channels[1][bin][pulse] - expected1).norm() <= tol * 1.5);
        }
    }
}
