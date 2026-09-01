//! Prueba de extremo a extremo del adapter UDP real, incluida la detección
//! de pérdida cuando un datagrama nunca llega (UDP no la garantiza).

use lamula_ingest::{decode_ray_frame, RadialAssembler};
use lamula_simulator::{generate_cell, pack_rays, CellParams, RayHeaderFields};
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::net::UdpSocket;

const FULL_SCALE: i16 = i16::MAX;

#[tokio::test]
async fn frames_arrive_over_udp() {
    const BINS: usize = 1;
    const M: usize = 5;
    let params = CellParams {
        power_s: 1.0,
        mean_v: 2.0,
        sigma_v: 0.5,
        wavelength_m: 0.10,
        prt_s: 1.0e-3,
        m: M,
        noise_floor: 0.0,
    };
    let mut rng = StdRng::seed_from_u64(99);
    let cells: Vec<_> = (0..BINS)
        .map(|_| generate_cell(&params, &mut rng))
        .collect();
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
        channel_mask: 0b0001,
        ray_flags: 0,
    };
    let wire_frames = pack_rays(&fields, &[cells], FULL_SCALE);

    let socket = lamula_ingest::udp::bind("127.0.0.1:0").await.unwrap();
    let local_addr = socket.local_addr().unwrap();
    let mut source = lamula_ingest::udp::spawn(socket, FULL_SCALE, 16);

    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for raw in &wire_frames {
        client.send_to(raw, local_addr).await.unwrap();
    }

    for raw in &wire_frames {
        let expected = decode_ray_frame(raw, FULL_SCALE).unwrap();
        let got = source.frames.recv().await.expect("trama");
        assert_eq!(got, expected);
    }
}

#[tokio::test]
async fn dropped_datagram_is_reflected_as_dropped_pulses_not_fabricated() {
    const BINS: usize = 1;
    const M: usize = 5;
    let params = CellParams {
        power_s: 1.0,
        mean_v: 2.0,
        sigma_v: 0.5,
        wavelength_m: 0.10,
        prt_s: 1.0e-3,
        m: M,
        noise_floor: 0.0,
    };
    let mut rng = StdRng::seed_from_u64(17);
    let cells: Vec<_> = (0..BINS)
        .map(|_| generate_cell(&params, &mut rng))
        .collect();
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
        channel_mask: 0b0001,
        ray_flags: 0,
    };
    let wire_frames = pack_rays(&fields, &[cells], FULL_SCALE);

    let socket = lamula_ingest::udp::bind("127.0.0.1:0").await.unwrap();
    let local_addr = socket.local_addr().unwrap();
    let mut source = lamula_ingest::udp::spawn(socket, FULL_SCALE, 16);

    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for (i, raw) in wire_frames.iter().enumerate() {
        if i == 2 {
            continue; // datagrama perdido a propósito, UDP no lo garantiza.
        }
        client.send_to(raw, local_addr).await.unwrap();
    }

    let mut assembler = RadialAssembler::new(M as u16);
    for _ in 0..(M - 1) {
        let frame = source.frames.recv().await.expect("trama");
        assembler.feed(frame).unwrap();
    }

    assert_eq!(
        assembler.dropped_pulses(),
        1,
        "el hueco de seq por el datagrama perdido debe quedar contado, no relleno"
    );
}
