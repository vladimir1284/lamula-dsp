//! Semilla de endurance/soak (`docs/dsp-plan.md` §10 "Endurance/soak runs
//! (long unattended processing at worst-case PRF/range) in Phase 4").
//!
//! Esto NO es un soak real (horas de proceso desatendido contra hardware al
//! peor caso de PRF/rango) — es una prueba acotada, reproducible en segundos
//! en este entorno, que corre miles de radiales sintéticos de punta a punta
//! por el pipeline real: `pack_rays` (cable) → `decode_ray_frame` →
//! `RadialAssembler` (la misma ingesta que usa `crates/ingest::tcp`) →
//! `build_moment_ray`, variando gate count, filtro de clutter y RFI en cada
//! vuelta. Detecta lo que SÍ es detectable sin hardware ni horas: panics,
//! `ray_seq` roto, o un momento publicado como infinito (la censura
//! explícita produce NaN a propósito — eso no es una falla; un `±inf` sí lo
//! sería, ninguna fórmula de este pipeline debería producirlo).
//!
//! Lo que esto NO prueba, y sigue pendiente de Fase 4: horas de duración
//! real, la PRF/rango efectivamente peor caso del hardware objetivo (no
//! decidido, ver `docs/dsp-plan.md` §8.2 Fase 0), fugas de memoria (sin
//! herramienta de instrumentación en este entorno) ni el binario real vía
//! TCP (eso ya lo cubre `tests/end_to_end.rs`, a una sola conexión).

use lamula_contract::dsp_rcp::{clutter_filter, dealias_mode, moment_kind, Config};
use lamula_dsp_service::ray::{build_moment_ray, PreviousPrf};
use lamula_ingest::{decode_ray_frame, RadialAssembler};
use lamula_rcp_link::wire::UpMessage;
use lamula_simulator::{generate_cell, pack_rays, CellParams, RayHeaderFields};
use rand::rngs::StdRng;
use rand::SeedableRng;

const FULL_SCALE: i16 = i16::MAX;
const ITERATIONS: usize = 2000;

#[test]
fn pipeline_survives_thousands_of_varied_radials_without_panicking_or_publishing_infinities() {
    let mut rng = StdRng::seed_from_u64(20260905);
    let mut previous_prf: Option<PreviousPrf> = None;
    let mut last_seq_start: Option<u32> = None;

    for i in 0..ITERATIONS {
        let n_gates = 1 + (i % 50);
        let n_pulses = 32usize;
        let cell = CellParams {
            power_s: 1.0,
            mean_v: (i % 20) as f64 - 10.0,
            sigma_v: 1.0 + (i % 3) as f64,
            wavelength_m: 0.10,
            prt_s: 1.0e-3,
            m: n_pulses,
            noise_floor: 0.02,
        };
        let cells: Vec<_> = (0..n_gates)
            .map(|_| generate_cell(&cell, &mut rng))
            .collect();
        let fields = RayHeaderFields {
            seq_start: (i as u32).wrapping_mul(n_pulses as u32),
            timestamp_ns_start: i as u64 * 1_000_000,
            timestamp_step_ns: 1000,
            trigger_count_start: i as u32,
            azimuth_raw: (i as u32 * 37) % 4096,
            elevation_raw: 0,
            prf_div: 1,
            pulse_width_idx: 0,
            pulse_mode: 0,
            cell_mode: 0,
            channel_mask: 0b0001,
            ray_flags: 0,
        };
        let wire_frames = pack_rays(&fields, &[cells], FULL_SCALE);

        let mut assembler = RadialAssembler::new(n_pulses as u16);
        let mut radial = None;
        for raw in &wire_frames {
            let frame =
                decode_ray_frame(raw, FULL_SCALE).expect("trama válida generada por este test");
            if let Some(r) = assembler
                .feed(frame)
                .expect("sin huecos de seq en este escenario")
            {
                radial = Some(r);
            }
        }
        let radial = radial.expect("un radial completo por vuelta");

        let config = Config {
            seq: i as u32,
            moment_mask: (1 << moment_kind::UZ)
                | (1 << moment_kind::CZ)
                | (1 << moment_kind::V)
                | (1 << moment_kind::SQI)
                | (1 << moment_kind::SIG),
            n_pulses: n_pulses as u16,
            n_gates: n_gates as u16,
            clutter_filter: if i % 2 == 0 {
                clutter_filter::GMAP
            } else {
                clutter_filter::NONE
            },
            dealias_mode: dealias_mode::NONE,
            sweep_mode: 0,
            estimator: 0,
            rfi_filter: (i % 3 == 0) as u8,
            range_dealias: 0,
            prf_ratio_num: 0,
            prf_ratio_den: 0,
            start_range_m: 0.0,
            gate_spacing_m: 250.0,
            prf_hz: 1000.0,
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
            antenna_isolation_db: 0.0,
            wavelength_m: 0.10,
            polarization_mode: 0,
            pad0: 0,
            burst_window_bins: 0,
        };

        let (msg, next_previous_prf) = build_moment_ray(
            &radial,
            &config,
            i as u32,
            i == 0,
            4096,
            0.0,
            previous_prf.as_ref(),
        );
        previous_prf = Some(next_previous_prf);

        let UpMessage::MomentRay { ray, moments } = &msg else {
            panic!("se esperaba MomentRay en la vuelta {i}");
        };
        let ray_seq = ray.seq; // copia local: `MomentRay` es `packed`
        assert_eq!(
            ray_seq, i as u32,
            "ray_seq debe ser monótono y coincidir con lo pedido"
        );
        assert!(!moments.is_empty(), "vuelta {i}: sin bloques de momentos");
        for block in moments {
            for &v in &block.values {
                assert!(
                    !v.is_infinite(),
                    "vuelta {i}, moment_kind {}: valor infinito",
                    block.field.kind
                );
            }
        }

        if let Some(prev) = last_seq_start {
            assert!(
                fields.seq_start >= prev,
                "seq_start debería ser no decreciente entre vueltas"
            );
        }
        last_seq_start = Some(fields.seq_start);
    }
}
