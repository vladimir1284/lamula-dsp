//! Hito M1 de punta a punta (`docs/dsp-plan.md:218`): simulador → AAL
//! (adapter en memoria) → `RadialAssembler` → `lamula_moments::pulse_pair_moments`,
//! comparando lo recuperado contra la verdad-terreno de `CellParams`. Es la
//! prueba de que la forma `channels[c][bin]` que entrega `AssembledRadial`
//! encaja sin adaptar con lo que ya consumen los crates de algoritmo.

use lamula_ingest::RadialAssembler;
use lamula_moments::pulse_pair_moments;
use lamula_simulator::{generate_cell, pack_rays, CellParams, RayHeaderFields};
use rand::rngs::StdRng;
use rand::SeedableRng;

const FULL_SCALE: i16 = i16::MAX;

#[tokio::test]
async fn simulator_to_moments_recovers_ground_truth_velocity() {
    const M: usize = 64;
    let params = CellParams {
        // Pequeña a propósito: evita que `quantize()` sature con la
        // envolvente Rayleigh de la señal (ver el comentario homólogo en
        // `tests/roundtrip.rs`) y así el estimador ve la señal cuantizada,
        // no una recortada.
        power_s: 0.01,
        mean_v: -12.5,
        sigma_v: 1.0,
        wavelength_m: 0.10,
        prt_s: 1.0e-3,
        m: M,
        noise_floor: 0.0,
    };
    let mut rng = StdRng::seed_from_u64(2026);
    let cell = generate_cell(&params, &mut rng);

    let fields = RayHeaderFields {
        seq_start: 0,
        timestamp_ns_start: 0,
        timestamp_step_ns: (params.prt_s * 1.0e9) as u64,
        trigger_count_start: 0,
        azimuth_raw: 0,
        elevation_raw: 0,
        prf_div: 4,
        pulse_width_idx: 0,
        pulse_mode: 0,
        cell_mode: 0,
        channel_mask: 0b0001,
        ray_flags: 0,
    };
    let wire_frames = pack_rays(&fields, &[vec![cell]], FULL_SCALE);

    let mut source = lamula_ingest::simulator::spawn(wire_frames, FULL_SCALE, M + 1);

    let mut assembler = RadialAssembler::new(M as u16);
    let mut radial = None;
    while radial.is_none() {
        let frame = source.frames.recv().await.expect("trama");
        radial = assembler.feed(frame).unwrap();
    }
    let radial = radial.unwrap();
    source.task.await.unwrap().unwrap();

    assert_eq!(radial.dropped_pulses, 0);
    let estimate = pulse_pair_moments(&radial.channels[0][0], params.wavelength_m, params.prt_s);

    assert!(
        (estimate.velocity_mps - params.mean_v).abs() < 1.0,
        "velocidad recuperada {} lejos de la verdad-terreno {}",
        estimate.velocity_mps,
        params.mean_v
    );
}
