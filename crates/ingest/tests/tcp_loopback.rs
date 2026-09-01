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
    drop(client); // cierre limpio: el adapter reconecta, no termina la tarea.

    for (i, raw) in wire_frames.iter().enumerate() {
        let expected = decode_ray_frame(raw, FULL_SCALE).unwrap();
        let got = source
            .frames
            .recv()
            .await
            .unwrap_or_else(|| panic!("falta la trama {i}"));
        assert_eq!(got, expected, "trama {i} fuera de orden o corrupta");
    }
    // Sin una conexión nueva no debería llegar nada más; el canal no se
    // cierra (el adapter sigue esperando en `listener.accept()`), así que
    // se comprueba con un timeout en vez de esperar un `None`.
    let extra = tokio::time::timeout(std::time::Duration::from_millis(200), source.frames.recv())
        .await;
    assert!(
        extra.is_err(),
        "no debería haber más tramas sin una conexión nueva"
    );
    source.task.abort();
}

#[tokio::test]
async fn reconnects_after_client_disconnects() {
    const BINS: usize = 1;
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
    let mut rng = StdRng::seed_from_u64(7);
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

    let mut client1 = TcpStream::connect(local_addr).await.unwrap();
    client1.write_all(&wire_frames[0]).await.unwrap();
    let got1 = source.frames.recv().await.expect("falta la trama del primer cliente");
    assert_eq!(got1, decode_ray_frame(&wire_frames[0], FULL_SCALE).unwrap());
    drop(client1);

    // El adapter no murió al desconectarse el primero: acepta un segundo
    // cliente sin reiniciar el proceso.
    let mut client2 = TcpStream::connect(local_addr).await.unwrap();
    client2.write_all(&wire_frames[0]).await.unwrap();
    let got2 = source.frames.recv().await.expect("falta la trama del segundo cliente");
    assert_eq!(got2, decode_ray_frame(&wire_frames[0], FULL_SCALE).unwrap());

    source.task.abort();
}
