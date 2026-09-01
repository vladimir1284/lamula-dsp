//! Prueba de extremo a extremo del adapter TCP real: un cliente de prueba se
//! conecta al listener de `lamula_ingest::tcp` y manda tramas generadas por
//! `pack_rays`; se comprueba que llegan en orden por el canal.

use lamula_ingest::decode_ray_frame;
use lamula_simulator::{generate_cell, pack_rays, CellParams, RayHeaderFields};
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

const FULL_SCALE: i16 = i16::MAX;

#[tokio::test]
async fn frames_arrive_in_order_over_tcp() {
    const BINS: usize = 2;
    const M: usize = 6;
    let params = CellParams {
        power_s: 1.0,
        mean_v: 3.0,
        sigma_v: 0.5,
        wavelength_m: 0.10,
        prt_s: 1.0e-3,
        m: M,
        noise_floor: 0.0,
    };
    let mut rng = StdRng::seed_from_u64(42);
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

    let listener = lamula_ingest::tcp::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let mut source = lamula_ingest::tcp::spawn(listener, FULL_SCALE, 16);

    let mut client = TcpStream::connect(local_addr).await.unwrap();
    for raw in &wire_frames {
        client.write_all(raw).await.unwrap();
    }
    drop(client); // cierra la conexión: el adapter debe terminar limpio.

    for (i, raw) in wire_frames.iter().enumerate() {
        let expected = decode_ray_frame(raw, FULL_SCALE).unwrap();
        let got = source
            .frames
            .recv()
            .await
            .unwrap_or_else(|| panic!("falta la trama {i}"));
        assert_eq!(got, expected, "trama {i} fuera de orden o corrupta");
    }
    assert!(
        source.frames.recv().await.is_none(),
        "no debería haber más tramas"
    );
    source.task.await.unwrap().unwrap();
}
