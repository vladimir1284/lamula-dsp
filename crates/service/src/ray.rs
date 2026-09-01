//! Construye un `MomentRay` a partir de un radial ya ensamblado, igual que
//! `crates/rcp-link/tests/vertical_slice.rs` pero con los campos que ahí
//! eran valores fijos de prueba (`start_range_m`, `gate_spacing_m`,
//! `noise_floor_dbm`, `radar_constant_db`) tomados aquí del `config` real
//! aplicado a la sesión, no inventados.
//!
//! Sólo UZ (sin corregir) y V: ver el doc-comment de `crate` para por qué
//! (`pulse_pair_moments` es el único estimador conectado a este binario).
//! `acq_time_utc_ns`/`acq_monotonic_ns` usan el mismo `timestamp_ns` del
//! DRx para los dos campos: el contrato `DRx↔DSP` sólo documenta ese campo
//! como "instante del trigger, reloj del DRx", sin confirmar que sea época
//! UTC — la misma simplificación que ya hace el vertical slice.

use lamula_contract::dsp_rcp::{data_type, moment_kind, ray_flag, Config, MomentField, MomentRay};
use lamula_ingest::{ssi_counts_to_deg, AssembledRadial};
use lamula_moments::pulse_pair_moments;
use lamula_rcp_link::wire::{MomentBlock, UpMessage};

const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

pub fn build_moment_ray(
    radial: &AssembledRadial,
    config: &Config,
    seq: u32,
    first_after_config: bool,
    ssi_counts_per_turn: u32,
    ssi_zero_offset_deg: f64,
) -> UpMessage {
    let wavelength_m = config.wavelength_m as f64;
    let prf_hz = config.prf_hz as f64;
    let prt_s = 1.0 / prf_hz;

    // `pulse_pair_moments` sólo corre sobre el canal 0: ver `capabilities`
    // en `crate::main` (`n_rx_channels: 1`), único canal que este binario
    // declara saber procesar.
    let estimates: Vec<_> = radial.channels[0]
        .iter()
        .map(|series| pulse_pair_moments(series, wavelength_m, prt_s))
        .collect();
    let n_gates = estimates.len() as u16;

    let uz_values: Vec<f32> = estimates
        .iter()
        .map(|e| (10.0 * e.s_linear.log10()) as f32)
        .collect();
    let v_values: Vec<f32> = estimates.iter().map(|e| e.velocity_mps as f32).collect();

    let az_start_deg =
        ssi_counts_to_deg(radial.azimuth_raw, ssi_counts_per_turn, ssi_zero_offset_deg) as f32;
    let el_start_deg = ssi_counts_to_deg(
        radial.elevation_raw,
        ssi_counts_per_turn,
        ssi_zero_offset_deg,
    ) as f32;

    let mut ray_flags = 0u8;
    if first_after_config {
        ray_flags |= ray_flag::FIRST_AFTER_CONFIG;
    }

    let mut moments = Vec::with_capacity(2);
    if config.moment_mask & (1 << moment_kind::UZ) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::UZ,
                data_type: data_type::F32,
                flags: 0,
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: uz_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::V) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::V,
                data_type: data_type::F32,
                flags: 0,
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: v_values,
        });
    }

    let ray = MomentRay {
        seq,
        acq_time_utc_ns: radial.timestamp_ns_start,
        acq_monotonic_ns: radial.timestamp_ns_start,
        // Sin controlador de antena en este repo: un solo radial estático
        // por barrido/volumen, todos con índice 0.
        volume_seq: 0,
        sweep_seq: 0,
        ray_index: 0,
        n_gates,
        n_pulses: config.n_pulses,
        bins_valid: n_gates,
        n_moments: moments.len() as u8,
        sweep_mode: config.sweep_mode,
        prf_mode: config.dealias_mode,
        ray_flags,
        pad0: 0,
        az_start_deg,
        az_end_deg: az_start_deg,
        el_start_deg,
        el_end_deg: el_start_deg,
        fixed_angle_deg: el_start_deg,
        start_range_m: config.start_range_m,
        gate_spacing_m: config.gate_spacing_m,
        prf_hz: config.prf_hz,
        nyquist_velocity: (wavelength_m / (4.0 * prt_s)) as f32,
        unambiguous_range_m: (SPEED_OF_LIGHT_M_S / (2.0 * prf_hz)) as f32,
        noise_floor_dbm: config.noise_floor_dbm,
        radar_constant_db: config.radar_constant_db,
    };

    UpMessage::MomentRay { ray, moments }
}
