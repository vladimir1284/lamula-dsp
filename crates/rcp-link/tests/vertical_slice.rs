//! Hito M1 completo (`docs/dsp-plan.md:218`): simulador → AAL (adapter en
//! memoria) → `RadialAssembler` → `pulse_pair_moments` por celda →
//! `MomentRay`/`MomentBlock` → `lamula_rcp_link::tcp` → un cliente TCP que
//! hace de RCP simulado, decodificando a mano (un RCP real decodifica desde
//! `contract/generated/dsp_rcp_v0_1.{py,ts}`, no desde este crate). Cierra el
//! hueco que dejaba `crates/ingest/tests/vertical_slice.rs`, que llegaba sólo
//! hasta `pulse_pair_moments` sin tocar el enlace RCP.
//!
//! Simplificaciones explícitas de esta prueba, no del pipeline real:
//! - Reflectividad *sin corregir* (UZ): `10·log10(s_linear)`, sin constante
//!   de radar ni corrección — es literalmente lo que pide el hito M1 ("un
//!   rayo de reflectividad sin corregir"). La calibración a dBZ real vive en
//!   `crates/calibration`, fuera de esta prueba.
//! - Un solo radial estático: no hay barrido simulado, así que
//!   `az_end_deg`/`el_end_deg` valen lo mismo que `az_start_deg`/`el_start_deg`.
//! - `counts_per_turn`/`zero_offset_deg` del encoder SSI son un valor de
//!   prueba arbitrario (ver `lamula_ingest::angle`, que documenta que esa
//!   configuración no tiene dueño todavía).
//! - `start_range_m`/`gate_spacing_m`/`noise_floor_dbm`/`radar_constant_db`
//!   no los modela el simulador de celdas (`generate_cell` no tiene noción de
//!   rango físico ni de calibración): van a un valor de prueba fijo, no
//!   calculado.

use lamula_contract::dsp_rcp::{
    self, data_type, dealias_mode, moment_kind, sweep_mode, MomentField, MomentRay, MsgType,
    HEADER_SIZE, MAGIC, MOMENT_FIELD_SIZE, MOMENT_RAY_SIZE,
};
use lamula_ingest::{ssi_counts_to_deg, RadialAssembler};
use lamula_moments::pulse_pair_moments;
use lamula_rcp_link::wire::{MomentBlock, UpMessage};
use lamula_simulator::{generate_cell, pack_rays, CellParams, RayHeaderFields};
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

const FULL_SCALE: i16 = i16::MAX;
const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

#[tokio::test]
async fn moment_stream_reaches_simulated_rcp_consumer() {
    const M: usize = 64;
    const N_GATES: usize = 3;
    let wavelength_m = 0.10;
    let prt_s = 1.0e-3;
    let ground_truth_v = [-12.5_f64, 3.0, 8.2];

    let mut rng = StdRng::seed_from_u64(2026);
    let cells: Vec<_> = ground_truth_v
        .iter()
        .map(|&mean_v| {
            let params = CellParams {
                power_s: 0.01,
                mean_v,
                sigma_v: 1.0,
                wavelength_m,
                prt_s,
                m: M,
                noise_floor: 0.0,
            };
            generate_cell(&params, &mut rng)
        })
        .collect();

    let counts_per_turn = 4096u32;
    let zero_offset_deg = 0.0;
    let azimuth_raw = 512u32; // 512/4096 * 360 = 45 grados
    let elevation_raw = 0u32;

    let fields = RayHeaderFields {
        seq_start: 0,
        timestamp_ns_start: 0,
        timestamp_step_ns: (prt_s * 1.0e9) as u64,
        trigger_count_start: 0,
        azimuth_raw,
        elevation_raw,
        prf_div: 4,
        pulse_width_idx: 0,
        pulse_mode: 0,
        cell_mode: 0,
        channel_mask: 0b0001,
        ray_flags: 0,
    };
    let wire_frames = pack_rays(&fields, &[cells], FULL_SCALE);

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
    assert_eq!(radial.channels[0].len(), N_GATES);

    let estimates: Vec<_> = radial.channels[0]
        .iter()
        .map(|series| pulse_pair_moments(series, wavelength_m, prt_s))
        .collect();

    let uz_values: Vec<f32> = estimates
        .iter()
        .map(|e| (10.0 * e.s_linear.log10()) as f32)
        .collect();
    let v_values: Vec<f32> = estimates.iter().map(|e| e.velocity_mps as f32).collect();

    let az_start_deg = ssi_counts_to_deg(azimuth_raw, counts_per_turn, zero_offset_deg) as f32;
    let el_start_deg = ssi_counts_to_deg(elevation_raw, counts_per_turn, zero_offset_deg) as f32;
    let prf_hz = 1.0 / prt_s;

    let ray = MomentRay {
        seq: 1,
        acq_time_utc_ns: radial.timestamp_ns_start,
        acq_monotonic_ns: radial.timestamp_ns_start,
        volume_seq: 0,
        sweep_seq: 0,
        ray_index: 0,
        n_gates: N_GATES as u16,
        n_pulses: M as u16,
        bins_valid: N_GATES as u16,
        n_moments: 2,
        sweep_mode: sweep_mode::PPI,
        prf_mode: dealias_mode::NONE,
        ray_flags: 0,
        pad0: 0,
        az_start_deg,
        az_end_deg: az_start_deg,
        el_start_deg,
        el_end_deg: el_start_deg,
        fixed_angle_deg: el_start_deg,
        start_range_m: 0.0,
        gate_spacing_m: 250.0,
        prf_hz: prf_hz as f32,
        nyquist_velocity: (wavelength_m / (4.0 * prt_s)) as f32,
        unambiguous_range_m: (SPEED_OF_LIGHT_M_S / (2.0 * prf_hz)) as f32,
        noise_floor_dbm: 0.0,
        radar_constant_db: 0.0,
    };

    let moments = vec![
        MomentBlock {
            field: MomentField {
                kind: moment_kind::UZ,
                data_type: data_type::F32,
                flags: 0,
                pad0: 0,
                n_gates: N_GATES as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: uz_values.clone(),
        },
        MomentBlock {
            field: MomentField {
                kind: moment_kind::V,
                data_type: data_type::F32,
                flags: 0,
                pad0: 0,
                n_gates: N_GATES as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: v_values.clone(),
        },
    ];

    let listener = lamula_rcp_link::tcp::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let link = lamula_rcp_link::tcp::spawn(listener, 4, 4);

    let mut rcp_client = TcpStream::connect(local_addr).await.unwrap();

    link.up
        .send(UpMessage::MomentRay { ray, moments })
        .await
        .unwrap();

    let mut header = [0u8; HEADER_SIZE];
    rcp_client.read_exact(&mut header).await.unwrap();
    let magic = u32::from_le_bytes(header[0..4].try_into().unwrap());
    assert_eq!(magic, MAGIC);
    assert_eq!(header[6], MsgType::MomentRay as u8);
    let payload_len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;

    let mut payload = vec![0u8; payload_len];
    rcp_client.read_exact(&mut payload).await.unwrap();

    // Cabecera fija del moment_ray: 88 B (ver `MOMENT_RAY_SIZE`).
    let got_n_gates = u16::from_le_bytes(payload[28..30].try_into().unwrap());
    assert_eq!(got_n_gates, N_GATES as u16);
    let got_az_start = f32::from_le_bytes(payload[40..44].try_into().unwrap());
    assert!((got_az_start - az_start_deg).abs() < 1e-6);

    // Los dos bloques de momento empiezan justo tras MOMENT_RAY_SIZE.
    let mut offset = MOMENT_RAY_SIZE;
    let mut decoded_uz = Vec::new();
    let mut decoded_v = Vec::new();
    for _ in 0..2 {
        let kind = payload[offset];
        let block_n_gates =
            u32::from_le_bytes(payload[offset + 4..offset + 8].try_into().unwrap()) as usize;
        assert_eq!(block_n_gates, N_GATES);
        let values_start = offset + MOMENT_FIELD_SIZE;
        let mut values = Vec::with_capacity(block_n_gates);
        for i in 0..block_n_gates {
            let v_off = values_start + i * 4;
            values.push(f32::from_le_bytes(
                payload[v_off..v_off + 4].try_into().unwrap(),
            ));
        }
        if kind == dsp_rcp::moment_kind::UZ {
            decoded_uz = values;
        } else if kind == dsp_rcp::moment_kind::V {
            decoded_v = values;
        }
        offset = values_start + block_n_gates * 4;
    }
    assert_eq!(offset, payload_len);

    assert_eq!(decoded_uz, uz_values);
    assert_eq!(decoded_v, v_values);
    for (recovered, truth) in decoded_v.iter().zip(ground_truth_v.iter()) {
        assert!(
            (*recovered as f64 - truth).abs() < 1.0,
            "velocidad recuperada {} lejos de la verdad-terreno {}",
            recovered,
            truth
        );
    }

    drop(link.up);
    drop(rcp_client);
    link.task.await.unwrap().unwrap();
}
