//! Construye un `MomentRay` a partir de un radial ya ensamblado, igual que
//! `crates/rcp-link/tests/vertical_slice.rs` pero con los campos que ahí
//! eran valores fijos de prueba (`start_range_m`, `gate_spacing_m`,
//! `noise_floor_dbm`, `radar_constant_db`) tomados aquí del `config` real
//! aplicado a la sesión, no inventados.
//!
//! Momentos: UZ (sin corregir), V, SQI y SIG del canal 0 (pulse-pair), más
//! ZDR/ρHV/ΦDP/KDP cuando el radial trae un segundo canal
//! (`lamula_polarimetry` en modo simultáneo/STAR, `lamula_kdp` sobre el ΦDP
//! resultante) — ver el doc-comment de `crate` para por qué no hay más (CCOR
//! no se publica porque no hay filtro de clutter conectado). `acq_time_utc_ns`/
//! `acq_monotonic_ns` usan el mismo `timestamp_ns` del DRx para los dos
//! campos: el contrato `DRx↔DSP` sólo documenta ese campo como "instante del
//! trigger, reloj del DRx", sin confirmar que sea época UTC — la misma
//! simplificación que ya hace el vertical slice.
//!
//! Censura (`docs/algorithms/ruido-y-umbrales.md` §"Umbrales"): se evalúan
//! `sig_threshold`, `sqi_threshold` y `log_threshold` sobre cada celda; si
//! cualquiera de los tres dispara, UZ y V de esa celda se codifican como
//! NaN (`moment_flag::HAS_MISSING` en su bloque, `ray_flag::CENSORED` en el
//! radial) — "se censura el momento publicado, no la muestra de entrada".
//! SQI y SIG en sí NUNCA se censuran por estos umbrales: son el índice que
//! explica por qué la celda se descartó, y ocultarlo sería contradecir la
//! razón de publicarlo (misma página, última frase de esa sección). SIG sí
//! puede salir NaN cuando no está matemáticamente definido (`s_linear <= 0`,
//! ver `lamula_quality::sig_db`) — eso no es censura por umbral, es un valor
//! indefinido, y también marca `HAS_MISSING`.
//! `ccor_threshold` no se aplica: no hay CCOR que evaluar sin filtro de
//! clutter conectado (hueco real, no un umbral que se ignore a propósito).
//!
//! ZDR/ρHV/ΦDP tienen su propia censura, independiente de la de arriba: si
//! `P_h` o `P_v` no superan `MIN_SNR_LIN_POLARIMETRIC` veces su propio ruido
//! (`lamula_polarimetry::PolarimetricFlag::Censored`), los tres salen NaN y
//! marcan `HAS_MISSING` en sus bloques — pero NO ponen `ray_flag::CENSORED`,
//! reservado a la censura de UZ/V de arriba (no hay una tercera cosa que
//! ese flag pueda distinguir). Si el radial sólo trae un canal, los tres
//! bloques simplemente no se publican aunque `moment_mask` los pida — no hay
//! canal V con que calcularlos.
//!
//! KDP (`lamula_kdp`, sólo con segundo canal, igual que ZDR/ρHV/ΦDP): se
//! calcula sobre el mismo perfil de ΦDP por celda, con dos pasos previos que
//! el propio crate delega en quien llama —
//! `docs/algorithms/kdp-estimacion.md` §"Parámetros del contrato que
//! consume"—: (1) censura adicional por ρHV bajo (`RHOHV_THRESHOLD_KDP`,
//! **sin respaldo documentado en este repo** — ver el comentario de esa
//! constante) y (2) desdoblado (`lamula_kdp::unwrap_deg`) antes del ajuste de
//! ventana deslizante (`lamula_kdp::kdp_window_fit`, `KDP_WINDOW_GATES`). Una
//! celda con KDP indeterminado (borde de perfil sin suficientes puntos, o
//! con algún ΦDP de entrada NaN dentro de su ventana) sale NaN +
//! `HAS_MISSING`, sin marcar `ray_flag::CENSORED` — mismo criterio que
//! ZDR/ρHV/ΦDP.

use lamula_contract::dsp_rcp::{
    data_type, dealias_mode, moment_flag, moment_kind, ray_flag, Config, MomentField, MomentRay,
};
use lamula_dual_prf::{continuity_fix, dealias_dual_prf};
use lamula_ingest::{ssi_counts_to_deg, AssembledRadial};
use lamula_kdp::{kdp_window_fit, unwrap_deg};
use lamula_moments::{pulse_pair_moments, PulsePairEstimate};
use lamula_noise::{censored_by_sig_threshold, snr_db};
use lamula_polarimetry::{polarimetric_moments_simultaneous, PolarimetricFlag};
use lamula_quality::{sig_db, sqi};
use lamula_rcp_link::wire::{MomentBlock, UpMessage};

const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// Margen mínimo de SNR lineal para censurar ZDR/ρHV/ΦDP
/// (`lamula_polarimetry::polarimetric_moments_simultaneous`). El valor de
/// referencia es el que usa `tools/oracles/polarimetria_covarianzas.ipynb`
/// (0.05) — `Config` (`contract/schema/dsp_rcp_v0_1.toml`) todavía no tiene
/// un campo propio para este margen, así que aquí queda fijo en vez de
/// inventarle un origen de configuración que no existe.
const MIN_SNR_LIN_POLARIMETRIC: f64 = 0.05;

/// Censura adicional de ΦDP por ρHV bajo antes de KDP
/// (`docs/algorithms/kdp-estimacion.md`, precondición del crate
/// `lamula-kdp`). **0.8 es una convención habitual en meteorología radar,
/// no un valor que este repositorio documente en ningún sitio** — ni la
/// página del algoritmo ni el oráculo (`tools/oracles/kdp_estimacion.ipynb`,
/// que barre ρHV 0.90/0.97/0.99 sin fijar un corte) dan una referencia
/// propia. Placeholder explícito hasta que exista un campo real en `Config`.
const RHOHV_THRESHOLD_KDP: f64 = 0.8;

/// Longitud de la ventana de ajuste de KDP, en celdas de rango. El propio
/// contrato v0.1 no tiene un campo para esto
/// (`docs/algorithms/kdp-estimacion.md` §"Parámetros del contrato que
/// consume": "hoy sería una constante de configuración local del DSP") — 15
/// celdas es el valor que usa el oráculo (`WINDOW_GATES`,
/// `tools/oracles/kdp_estimacion.ipynb`, ~2.25 km a 150 m de espaciado).
const KDP_WINDOW_GATES: usize = 15;

/// Radio de búsqueda de pliegues para `lamula_dual_prf::continuity_fix`. Sin
/// campo propio en `Config` (mismo motivo que `KDP_WINDOW_GATES`); 3 es el
/// valor del propio test de la función y del oráculo
/// (`tools/oracles/dual_prf_dealiasing.ipynb`, `continuity_fix`).
const DUAL_PRF_MAX_FOLD_SEARCH: i64 = 3;

/// `(zdr, rhohv, phidp, kdp)` por celda, sólo con segundo canal — ver el
/// doc-comment del módulo.
type PolarimetricValues = (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>);

/// Última medida de velocidad pulse-pair (sin desdoblar) de un radial, para
/// emparejarla con el siguiente en modo dual-PRF. `crate::main` la conserva
/// entre llamadas a [`build_moment_ray`]; se reinicia junto con el
/// ensamblador en `START`/`STOP`/`ENTER_SETUP` porque un emparejamiento con
/// un radial de antes de ese corte no tiene sentido físico.
///
/// `own_prt_s` es el PRT con el que se calculó `velocity_mps` en su momento
/// — no necesariamente el correcto: si ESE radial fue a su vez el primero
/// de una alternancia (rol desconocido), se calculó con `mean_prt_s`. Al
/// emparejarlo ahora, con el rol ya conocido por comparación de `prf_div`,
/// `velocity_mps` se reescala por `own_prt_s / prt_correcto` antes de
/// usarlo — la fase `arg(R1)` no depende de qué PRT se le haya aplicado
/// después, así que la velocidad medida escala linealmente con el PRT
/// asumido y esto recupera exactamente el valor que se habría obtenido con
/// el PRT correcto desde el principio, sin necesidad de guardar la fase
/// cruda.
pub struct PreviousPrf {
    pub prf_div: u32,
    pub own_prt_s: f64,
    pub velocity_mps: Vec<f32>,
}

/// `(prt_low_s, prt_high_s, v_a1, v_a2, v_ext)`: periodo y Nyquist de la PRF
/// baja/alta, más la Nyquist extendida dual-PRF, derivados sin conocer
/// `FS_HZ` del DRx (decisión D-02, ver el doc-comment de `crate`) a partir
/// de `prf_hz` —documentado como la *media* en dual-PRF— y
/// `prf_ratio_num`/`prf_ratio_den` (razón baja:alta). `k = 2·prf_hz /
/// (num+den)` da la unidad de la razón tal que `PRF_baja = num·k`,
/// `PRF_alta = den·k`; `v_ext = den·v_a1 == num·v_a2` por construcción
/// (comprobado contra el test de `lamula_dual_prf::dealias_dual_prf` en
/// `crates/dual-prf`, que usa una razón 2:3 y `v_ext = 3·v_a1`).
///
/// `prt_low_s`/`prt_high_s` importan tanto como `v_a1`/`v_a2`: el periodo
/// real de un radial en modo dual-PRF NO es `1/prf_hz` (la media) — usar la
/// media ahí escalaría mal la fase de `pulse_pair_moments` y produciría una
/// velocidad incorrecta, no simplemente plegada distinto. Ver el uso en
/// `build_moment_ray`.
fn dual_prf_split(config: &Config) -> (f64, f64, f64, f64, f64) {
    let wavelength_m = config.wavelength_m as f64;
    let num = config.prf_ratio_num as f64;
    let den = config.prf_ratio_den as f64;
    let k = 2.0 * config.prf_hz as f64 / (num + den);
    let prf_low = num * k;
    let prf_high = den * k;
    let v_a1 = wavelength_m * prf_low / 4.0;
    let v_a2 = wavelength_m * prf_high / 4.0;
    let v_ext = den * v_a1;
    (1.0 / prf_low, 1.0 / prf_high, v_a1, v_a2, v_ext)
}

/// Cantidades derivadas de un `PulsePairEstimate` que la censura y los
/// cuatro momentos publicados necesitan, calculadas una sola vez por celda.
struct GateQuality {
    uz_db: f64,
    sqi_value: Option<f64>,
    sig_value: Option<f64>,
    censored: bool,
}

fn gate_quality(e: &PulsePairEstimate, config: &Config) -> GateQuality {
    let uz_db = if e.s_linear > 0.0 {
        10.0 * e.s_linear.log10()
    } else {
        f64::NEG_INFINITY
    };
    let snr = if e.s_linear > 0.0 {
        snr_db(e.s_linear, e.noise_floor_estimate)
    } else {
        f64::NEG_INFINITY
    };
    // `sqi()` exige r0_raw > 0; sólo falla con una ráfaga exactamente cero,
    // que en datos reales no ocurre (siempre hay algo de ruido del
    // receptor) — se guarda igual para no entrar en pánico ante ese caso
    // degenerado.
    let sqi_value = (e.r0_raw > 0.0).then(|| sqi(e.r0_raw, e.r1_abs));
    let sig_value = sig_db(e.s_linear, e.noise_floor_estimate);

    let censored = censored_by_sig_threshold(snr, config.sig_threshold as f64)
        || sqi_value.map_or(true, |v| v < config.sqi_threshold as f64)
        || uz_db <= config.log_threshold as f64;

    GateQuality {
        uz_db,
        sqi_value,
        sig_value,
        censored,
    }
}

pub fn build_moment_ray(
    radial: &AssembledRadial,
    config: &Config,
    seq: u32,
    first_after_config: bool,
    ssi_counts_per_turn: u32,
    ssi_zero_offset_deg: f64,
    previous_prf: Option<&PreviousPrf>,
) -> (UpMessage, PreviousPrf) {
    let wavelength_m = config.wavelength_m as f64;
    let mean_prt_s = 1.0 / config.prf_hz as f64;

    // Dual-PRF: `radial.prf_div` sólo dice "distinto o igual que el
    // anterior" (D-02: sin `FS_HZ` no hay Hz absoluto que sacarle) — el rol
    // de ESTE radial (PRF baja o alta del par) sólo se conoce comparándolo
    // con el radial anterior. Sin ese punto de referencia (primer radial
    // tras `START`/config, o dos radiales seguidos con el mismo `prf_div` —
    // el DRx no alternó como se le pidió) no hay forma de saber a qué PRT
    // real corresponden sus pulsos: usar la media (`mean_prt_s`) ahí sería
    // adivinar la escala de fase, no sólo perder el desdoblado — por eso
    // ese caso también marca `ray_flag::DEALIAS_FAILED`, aunque sea el
    // *periodo* lo que falla y no sólo el pliegue.
    let mut dealias_failed = false;
    let dual_prf_role: Option<bool> = if config.dealias_mode == dealias_mode::DUAL_PRF {
        match previous_prf {
            // `true` = este radial es el de PRF baja (mayor `prf_div`).
            Some(prev) if prev.prf_div != radial.prf_div => {
                Some(radial.prf_div > prev.prf_div)
            }
            _ => {
                dealias_failed = true;
                None
            }
        }
    } else {
        None
    };
    let own_prt_s = match dual_prf_role {
        Some(is_low) => {
            let (prt_low, prt_high, ..) = dual_prf_split(config);
            if is_low {
                prt_low
            } else {
                prt_high
            }
        }
        None => mean_prt_s,
    };

    // UZ/V/SQI/SIG (pulse-pair) sólo corren sobre el canal 0 — H, por
    // convención del contrato. El canal 1 (V), cuando está presente, sólo
    // alimenta ZDR/ρHV/ΦDP/KDP más abajo.
    let estimates: Vec<_> = radial.channels[0]
        .iter()
        .map(|series| pulse_pair_moments(series, wavelength_m, own_prt_s))
        .collect();
    let n_gates = estimates.len() as u16;

    let quality: Vec<GateQuality> = estimates.iter().map(|e| gate_quality(e, config)).collect();
    let any_censored = quality.iter().any(|q| q.censored);

    let uz_values: Vec<f32> = quality
        .iter()
        .map(|q| if q.censored { f32::NAN } else { q.uz_db as f32 })
        .collect();
    let raw_velocity_mps: Vec<f32> = estimates.iter().map(|e| e.velocity_mps as f32).collect();

    // Con el rol conocido y el radial anterior del mismo número de celdas,
    // se reconcilian las dos medidas (cada una ya escalada con su propio
    // PRT real, ver arriba) y se aplica continuidad espacial. Sin eso, se
    // publica la velocidad tal cual (ya con la escala correcta si el rol se
    // conoció, aunque no se haya podido desdoblar). La convergencia por
    // celda (residuo de `dealias_dual_prf`) no se evalúa aparte: no hay
    // umbral documentado en este repo para "residuo aceptable", mismo tipo
    // de hueco que `RHOHV_THRESHOLD_KDP`.
    let dealiased_velocity_mps: Vec<f32> = match (dual_prf_role, previous_prf) {
        (Some(is_low), Some(prev)) if prev.velocity_mps.len() == raw_velocity_mps.len() => {
            let (prt_low, prt_high, v_a1, v_a2, v_ext) = dual_prf_split(config);
            // El rol de `prev` es el opuesto del de este radial (si no,
            // `prev.prf_div == radial.prf_div` y no se habría llegado aquí
            // — ver `dual_prf_role`). Se reescala su velocidad guardada al
            // PRT correcto de SU rol antes de usarla: puede haberse
            // calculado con `mean_prt_s` si `prev` fue a su vez un radial
            // aislado (ver el doc-comment de `PreviousPrf`).
            let prev_correct_prt_s = if is_low { prt_high } else { prt_low };
            let prev_velocity_rescaled: Vec<f64> = prev
                .velocity_mps
                .iter()
                .map(|&v| v as f64 * (prev.own_prt_s / prev_correct_prt_s))
                .collect();
            let v_hats: Vec<f64> = if is_low {
                raw_velocity_mps
                    .iter()
                    .zip(&prev_velocity_rescaled)
                    .map(|(&v1, &v2)| dealias_dual_prf(v1 as f64, v2, v_a1, v_a2, v_ext).velocity_mps)
                    .collect()
            } else {
                prev_velocity_rescaled
                    .iter()
                    .zip(&raw_velocity_mps)
                    .map(|(&v1, &v2)| dealias_dual_prf(v1, v2 as f64, v_a1, v_a2, v_ext).velocity_mps)
                    .collect()
            };
            continuity_fix(&v_hats, v_a1, DUAL_PRF_MAX_FOLD_SEARCH)
                .into_iter()
                .map(|v| v as f32)
                .collect()
        }
        (Some(_), Some(_)) => {
            // Rol conocido pero número de celdas distinto del radial
            // anterior: no se puede emparejar celda a celda.
            dealias_failed = true;
            raw_velocity_mps.clone()
        }
        _ => raw_velocity_mps.clone(),
    };

    let v_values: Vec<f32> = dealiased_velocity_mps
        .iter()
        .zip(&quality)
        .map(|(&v, q)| if q.censored { f32::NAN } else { v })
        .collect();
    // SQI y SIG nunca se censuran por umbral: ver el doc-comment del
    // módulo. Sólo salen NaN cuando la cantidad no está definida
    // (`Option::None` de `gate_quality`).
    let sqi_values: Vec<f32> = quality
        .iter()
        .map(|q| q.sqi_value.map(|v| v as f32).unwrap_or(f32::NAN))
        .collect();
    let sig_values: Vec<f32> = quality
        .iter()
        .map(|q| q.sig_value.map(|v| v as f32).unwrap_or(f32::NAN))
        .collect();

    // Sólo hay ρHV/ZDR/ΦDP/KDP con un segundo canal (V) presente en el
    // radial — `channel_mask`/`channels.len()` lo refleja tal cual llega
    // del DRx.
    let polarimetric: Option<PolarimetricValues> = (radial.channels.len() > 1)
        .then(|| {
            let mut zdr = Vec::with_capacity(n_gates as usize);
            let mut rhohv = Vec::with_capacity(n_gates as usize);
            let mut phidp = Vec::with_capacity(n_gates as usize);
            for (h, v) in radial.channels[0].iter().zip(radial.channels[1].iter()) {
                let est = polarimetric_moments_simultaneous(
                    h,
                    v,
                    config.zdr_offset_db as f64,
                    config.phidp_offset_deg as f64,
                    MIN_SNR_LIN_POLARIMETRIC,
                );
                let (z, r, p) = match est.flag {
                    PolarimetricFlag::Ok => {
                        (est.zdr_db as f32, est.rhohv as f32, est.phidp_deg as f32)
                    }
                    PolarimetricFlag::Censored => (f32::NAN, f32::NAN, f32::NAN),
                };
                zdr.push(z);
                rhohv.push(r);
                phidp.push(p);
            }

            // KDP: censura adicional por ρHV bajo, luego desdoblado y ajuste
            // de ventana sobre el ΦDP resultante — ver el doc-comment del
            // módulo para las dos constantes sin campo propio en `Config`.
            let phidp_for_kdp: Vec<f64> = phidp
                .iter()
                .zip(&rhohv)
                .map(|(&p, &r)| {
                    if r.is_nan() || (r as f64) < RHOHV_THRESHOLD_KDP {
                        f64::NAN
                    } else {
                        p as f64
                    }
                })
                .collect();
            let phidp_unwrapped = unwrap_deg(&phidp_for_kdp);
            let gate_spacing_km = config.gate_spacing_m as f64 / 1000.0;
            let kdp: Vec<f32> = kdp_window_fit(&phidp_unwrapped, gate_spacing_km, KDP_WINDOW_GATES)
                .into_iter()
                .map(|k| k.map(|v| v as f32).unwrap_or(f32::NAN))
                .collect();

            (zdr, rhohv, phidp, kdp)
        });

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
    if any_censored {
        ray_flags |= ray_flag::CENSORED;
    }
    if dealias_failed {
        ray_flags |= ray_flag::DEALIAS_FAILED;
    }

    let moment_flags = |values: &[f32]| -> u8 {
        if values.iter().any(|v| v.is_nan()) {
            moment_flag::HAS_MISSING
        } else {
            0
        }
    };

    let mut moments = Vec::with_capacity(8);
    if config.moment_mask & (1 << moment_kind::UZ) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::UZ,
                data_type: data_type::F32,
                flags: moment_flags(&uz_values),
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
                flags: moment_flags(&v_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: v_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::SQI) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::SQI,
                data_type: data_type::F32,
                flags: moment_flags(&sqi_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: sqi_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::SIG) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::SIG,
                data_type: data_type::F32,
                flags: moment_flags(&sig_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: sig_values,
        });
    }
    if let Some((zdr_values, rhohv_values, phidp_values, kdp_values)) = polarimetric {
        if config.moment_mask & (1 << moment_kind::ZDR) != 0 {
            moments.push(MomentBlock {
                field: MomentField {
                    kind: moment_kind::ZDR,
                    data_type: data_type::F32,
                    flags: moment_flags(&zdr_values),
                    pad0: 0,
                    n_gates: n_gates as u32,
                    scale: 1.0,
                    offset: 0.0,
                },
                values: zdr_values,
            });
        }
        if config.moment_mask & (1 << moment_kind::RHOHV) != 0 {
            moments.push(MomentBlock {
                field: MomentField {
                    kind: moment_kind::RHOHV,
                    data_type: data_type::F32,
                    flags: moment_flags(&rhohv_values),
                    pad0: 0,
                    n_gates: n_gates as u32,
                    scale: 1.0,
                    offset: 0.0,
                },
                values: rhohv_values,
            });
        }
        if config.moment_mask & (1 << moment_kind::PHIDP) != 0 {
            moments.push(MomentBlock {
                field: MomentField {
                    kind: moment_kind::PHIDP,
                    data_type: data_type::F32,
                    flags: moment_flags(&phidp_values),
                    pad0: 0,
                    n_gates: n_gates as u32,
                    scale: 1.0,
                    offset: 0.0,
                },
                values: phidp_values,
            });
        }
        if config.moment_mask & (1 << moment_kind::KDP) != 0 {
            moments.push(MomentBlock {
                field: MomentField {
                    kind: moment_kind::KDP,
                    data_type: data_type::F32,
                    flags: moment_flags(&kdp_values),
                    pad0: 0,
                    n_gates: n_gates as u32,
                    scale: 1.0,
                    offset: 0.0,
                },
                values: kdp_values,
            });
        }
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
        // Dual-PRF publica la Nyquist EXTENDIDA (doc del algoritmo,
        // §"Parámetros del contrato que consume"), no la de una sola PRF.
        nyquist_velocity: if config.dealias_mode == dealias_mode::DUAL_PRF {
            dual_prf_split(config).4 as f32
        } else {
            (wavelength_m / (4.0 * mean_prt_s)) as f32
        },
        unambiguous_range_m: (SPEED_OF_LIGHT_M_S / (2.0 * config.prf_hz as f64)) as f32,
        noise_floor_dbm: config.noise_floor_dbm,
        radar_constant_db: config.radar_constant_db,
    };

    let previous_prf_out = PreviousPrf {
        prf_div: radial.prf_div,
        own_prt_s,
        velocity_mps: raw_velocity_mps,
    };

    (UpMessage::MomentRay { ray, moments }, previous_prf_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamula_moments::PulsePairFlag;

    fn config_with_thresholds(
        sig_threshold: f32,
        sqi_threshold: f32,
        log_threshold: f32,
    ) -> Config {
        Config {
            seq: 1,
            moment_mask: 0,
            n_pulses: 64,
            n_gates: 1,
            clutter_filter: 0,
            dealias_mode: 0,
            sweep_mode: 0,
            estimator: 0,
            rfi_filter: 0,
            range_dealias: 0,
            prf_ratio_num: 0,
            prf_ratio_den: 0,
            start_range_m: 0.0,
            gate_spacing_m: 250.0,
            prf_hz: 1000.0,
            sqi_threshold,
            sig_threshold,
            ccor_threshold: 20.0,
            log_threshold,
            clutter_width_ms: 1.0,
            radar_constant_db: 65.0,
            noise_floor_dbm: -108.0,
            receiver_gain_db: 40.0,
            zdr_offset_db: 0.0,
            phidp_offset_deg: 0.0,
            wavelength_m: 0.10,
            pad0: 0,
        }
    }

    fn estimate(
        s_linear: f64,
        r0_raw: f64,
        r1_abs: f64,
        noise_floor_estimate: f64,
    ) -> PulsePairEstimate {
        PulsePairEstimate {
            s_linear,
            r0_raw,
            r1_abs,
            noise_floor_estimate,
            velocity_mps: 3.0,
            spectrum_width_mps: Some(1.0),
            flag: if s_linear > 0.0 {
                PulsePairFlag::Ok
            } else {
                PulsePairFlag::Censored
            },
        }
    }

    #[test]
    fn strong_coherent_signal_is_not_censored() {
        // S=1.0, N=0.01 -> SNR=20dB; r1_abs cerca de r0_raw -> SQI alto;
        // uz_db = 0dB, muy por encima de log_threshold.
        let e = estimate(1.0, 1.01, 0.95, 0.01);
        let config = config_with_thresholds(3.0, 0.4, -10.0);
        let q = gate_quality(&e, &config);
        assert!(!q.censored);
        assert!(q.sqi_value.unwrap() > 0.4);
        assert!(q.sig_value.unwrap() > 3.0);
    }

    #[test]
    fn low_snr_censors_but_still_publishes_sig() {
        // S=0.02, N=0.01 -> SNR=3.01dB, muy por debajo del umbral (10dB).
        let e = estimate(0.02, 1.03, 0.95, 0.01);
        let config = config_with_thresholds(10.0, 0.0, -100.0);
        let q = gate_quality(&e, &config);
        assert!(q.censored, "SNR bajo umbral debería censurar UZ/V");
        assert!(
            q.sig_value.is_some(),
            "SIG no se censura por umbral: sigue publicado aunque censure UZ/V"
        );
    }

    #[test]
    fn low_sqi_censors_even_with_good_snr() {
        // SNR alto (S=1.0, N=0.01) pero r1_abs pequeño frente a r0_raw ->
        // SQI bajo: censura por coherencia, no por SNR.
        let e = estimate(1.0, 1.01, 0.05, 0.01);
        let config = config_with_thresholds(3.0, 0.4, -100.0);
        let q = gate_quality(&e, &config);
        assert!(q.sqi_value.unwrap() < 0.4);
        assert!(q.censored, "SQI bajo umbral debería censurar UZ/V");
        assert!(q.sig_value.is_some(), "SIG sigue publicado");
    }

    #[test]
    fn cell_with_no_detectable_signal_has_undefined_sig() {
        let e = estimate(0.0, 0.01, 0.005, 0.01);
        let config = config_with_thresholds(3.0, 0.4, -10.0);
        let q = gate_quality(&e, &config);
        assert!(q.censored);
        assert!(
            q.sig_value.is_none(),
            "sin señal detectable SIG no está definido, no es 'censurado'"
        );
    }

    use lamula_simulator::{generate_cell, generate_dual_pol_cell, CellParams, DualPolParams};
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use rustfft::num_complex::Complex64;

    fn polarimetric_config(moment_mask: u32) -> Config {
        Config {
            moment_mask,
            zdr_offset_db: 0.0,
            phidp_offset_deg: 0.0,
            ..config_with_thresholds(3.0, 0.4, -100.0)
        }
    }

    fn radial_from_channels(channels: Vec<Vec<Vec<Complex64>>>) -> AssembledRadial {
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
            channel_mask: if channels.len() > 1 { 0b0011 } else { 0b0001 },
            channels,
            dropped_pulses: 0,
        }
    }

    const ALL_MOMENTS_MASK: u32 = (1 << moment_kind::UZ)
        | (1 << moment_kind::V)
        | (1 << moment_kind::SQI)
        | (1 << moment_kind::SIG)
        | (1 << moment_kind::ZDR)
        | (1 << moment_kind::RHOHV)
        | (1 << moment_kind::PHIDP)
        | (1 << moment_kind::KDP);

    #[test]
    fn dual_channel_radial_publishes_polarimetric_moments() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.0 / 1000.0,
            m: 64,
            noise_floor: 0.01,
        };
        let dual = DualPolParams {
            zdr_db: 2.0,
            rho_hv: 0.98,
            phidp_deg: 10.0,
        };
        let (h, v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
        let radial = radial_from_channels(vec![vec![h], vec![v]]);
        let config = polarimetric_config(ALL_MOMENTS_MASK);

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };

        for kind in [moment_kind::ZDR, moment_kind::RHOHV, moment_kind::PHIDP] {
            let block = moments
                .iter()
                .find(|m| m.field.kind == kind)
                .unwrap_or_else(|| panic!("falta el bloque de moment_kind {kind}"));
            assert!(
                block.values[0].is_finite(),
                "celda con SNR y coherencia altas no debería censurarse"
            );
        }
        // Un solo gate no llena ni el mínimo de 3 puntos de la ventana de
        // KDP: indeterminado, no censurado, pero igual sale NaN.
        let kdp_block = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::KDP)
            .expect("falta el bloque de KDP");
        assert!(kdp_block.values[0].is_nan());
    }

    #[test]
    fn single_channel_radial_omits_polarimetric_moments() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.0 / 1000.0,
            m: 64,
            noise_floor: 0.01,
        };
        let dual = DualPolParams {
            zdr_db: 2.0,
            rho_hv: 0.98,
            phidp_deg: 10.0,
        };
        let (h, _v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
        let radial = radial_from_channels(vec![vec![h]]);
        let config = polarimetric_config(ALL_MOMENTS_MASK);

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };

        for kind in [
            moment_kind::ZDR,
            moment_kind::RHOHV,
            moment_kind::PHIDP,
            moment_kind::KDP,
        ] {
            assert!(
                !moments.iter().any(|m| m.field.kind == kind),
                "sin canal V no hay con qué calcular moment_kind {kind}"
            );
        }
    }

    #[test]
    fn decorrelated_channels_censor_polarimetric_moments() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.0 / 1000.0,
            m: 64,
            noise_floor: 0.01,
        };
        // rho_hv = 0.0 no basta por sí solo para forzar la censura (que
        // depende del margen de SNR por canal, no de rho_hv): se baja
        // `power_s` muy por debajo del ruido para que sí dispare (con
        // margen amplio frente al ruido de estimación de `noise_floor`).
        let cell = CellParams {
            power_s: 0.0001,
            ..cell
        };
        let dual = DualPolParams {
            zdr_db: 0.0,
            rho_hv: 0.0,
            phidp_deg: 0.0,
        };
        let (h, v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
        let radial = radial_from_channels(vec![vec![h], vec![v]]);
        let config = polarimetric_config(ALL_MOMENTS_MASK);

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };

        for kind in [
            moment_kind::ZDR,
            moment_kind::RHOHV,
            moment_kind::PHIDP,
            moment_kind::KDP,
        ] {
            let block = moments
                .iter()
                .find(|m| m.field.kind == kind)
                .unwrap_or_else(|| panic!("falta el bloque de moment_kind {kind}"));
            assert!(
                block.values[0].is_nan(),
                "SNR por debajo del margen debería censurar ZDR/ρHV/ΦDP (y arrastrar KDP)"
            );
            assert_eq!(block.field.flags, moment_flag::HAS_MISSING);
        }
    }

    #[test]
    fn kdp_window_fit_recovers_slope_from_dual_channel_radial() {
        // Perfil de ΦDP lineal (KDP verdadero constante), igual que el test
        // de `lamula_kdp::kdp_window_fit`, pero generado a través de IQ
        // dual-pol real por celda en vez de un array analítico — comprueba
        // que el cableo (censura por ρHV, `unwrap_deg`, `kdp_window_fit`)
        // reconstruye una pendiente conocida, no la exactitud del algoritmo
        // en sí (eso ya lo cubre `crates/kdp/tests/against_oracle.rs`).
        const K0_DEG_PER_KM: f64 = 2.0;
        const GATE_SPACING_KM: f64 = 0.150;
        const N_GATES: usize = 30;

        let mut rng = StdRng::seed_from_u64(20260901);
        let mut h_channel = Vec::with_capacity(N_GATES);
        let mut v_channel = Vec::with_capacity(N_GATES);
        for i in 0..N_GATES {
            let cell = CellParams {
                power_s: 1.0,
                mean_v: 3.0,
                sigma_v: 1.0,
                wavelength_m: 0.10,
                prt_s: 1.0 / 1000.0,
                m: 64,
                noise_floor: 0.001,
            };
            let dual = DualPolParams {
                zdr_db: 0.0,
                rho_hv: 0.999,
                phidp_deg: 2.0 * K0_DEG_PER_KM * i as f64 * GATE_SPACING_KM,
            };
            let (h, v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
            h_channel.push(h);
            v_channel.push(v);
        }
        let radial = radial_from_channels(vec![h_channel, v_channel]);
        let mut config = polarimetric_config(ALL_MOMENTS_MASK);
        config.gate_spacing_m = (GATE_SPACING_KM * 1000.0) as f32;

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };

        let kdp_block = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::KDP)
            .expect("falta el bloque de KDP");
        // Celda central: ventana completa de 15, lejos de los dos bordes.
        let mid = kdp_block.values[N_GATES / 2];
        assert!(
            (mid as f64 - K0_DEG_PER_KM).abs() < 1.0,
            "KDP recuperado ({mid}) debería acercarse a K0={K0_DEG_PER_KM} deg/km"
        );
    }

    fn dual_prf_config() -> Config {
        // Misma razón 2:3 que el test de `lamula_dual_prf::dealias_dual_prf`
        // (`PRT1=1.2ms`, `PRT2=0.8ms`): PRF1≈833.33Hz, PRF2=1250Hz.
        let mean_prf_hz = (1.0 / 1.2e-3 + 1.0 / 0.8e-3) / 2.0;
        Config {
            dealias_mode: dealias_mode::DUAL_PRF,
            prf_hz: mean_prf_hz as f32,
            prf_ratio_num: 2,
            prf_ratio_den: 3,
            moment_mask: (1 << moment_kind::UZ) | (1 << moment_kind::V),
            ..config_with_thresholds(3.0, 0.4, -100.0)
        }
    }

    #[test]
    fn isolated_dual_prf_radial_marks_dealias_failed() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 25.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.2e-3, // PRF baja del par (833.33 Hz)
            m: 64,
            noise_floor: 0.01,
        };
        let h = generate_cell(&cell, &mut rng);
        let mut radial = radial_from_channels(vec![vec![h]]);
        radial.prf_div = 3; // mayor divisor = PRF baja

        let (msg, _) = build_moment_ray(&radial, &dual_prf_config(), 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { ray, .. } = msg else {
            panic!("se esperaba MomentRay");
        };
        assert_eq!(
            ray.ray_flags & ray_flag::DEALIAS_FAILED,
            ray_flag::DEALIAS_FAILED,
            "sin radial anterior no hay con qué emparejar"
        );
    }

    #[test]
    fn dual_prf_pair_unfolds_velocity_across_two_radials() {
        // Reproduce el escenario del test de `lamula_dual_prf::dealias_dual_prf`
        // (v_true=25 m/s, V_A1≈20.83, V_A2≈31.25) pero generando IQ real por
        // radial en vez de invocar `dealias_dual_prf` directo — comprueba el
        // cableo completo: PRT real por rol (no la media), emparejamiento por
        // `prf_div` y desdoblado.
        const V_TRUE: f64 = 25.0;
        let config = dual_prf_config();

        let mut rng = StdRng::seed_from_u64(20260901);
        let cell_low = CellParams {
            power_s: 1.0,
            mean_v: V_TRUE,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.2e-3, // PRF baja (833.33 Hz)
            m: 64,
            noise_floor: 0.01,
        };
        let h1 = generate_cell(&cell_low, &mut rng);
        let mut radial1 = radial_from_channels(vec![vec![h1]]);
        radial1.prf_div = 3; // mayor divisor = PRF baja

        let cell_high = CellParams {
            prt_s: 0.8e-3, // PRF alta (1250 Hz)
            ..cell_low
        };
        let h2 = generate_cell(&cell_high, &mut rng);
        let mut radial2 = radial_from_channels(vec![vec![h2]]);
        radial2.prf_div = 2; // menor divisor = PRF alta

        let (_, previous_prf) =
            build_moment_ray(&radial1, &config, 1, false, 1_000_000, 0.0, None);
        let (msg2, _) = build_moment_ray(
            &radial2,
            &config,
            2,
            false,
            1_000_000,
            0.0,
            Some(&previous_prf),
        );
        let UpMessage::MomentRay { ray, moments } = msg2 else {
            panic!("se esperaba MomentRay");
        };

        assert_eq!(
            ray.ray_flags & ray_flag::DEALIAS_FAILED,
            0,
            "segundo radial del par sí debería poder desdoblar"
        );
        let v_block = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::V)
            .expect("falta el bloque de V");
        assert!(
            (v_block.values[0] as f64 - V_TRUE).abs() < 2.0,
            "V desdoblado ({}) debería acercarse a v_true={V_TRUE}",
            v_block.values[0]
        );
    }
}
