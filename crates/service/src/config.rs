//! Parámetros de arranque del binario, todos por variable de entorno: nada
//! de esto tiene un valor documentado en el repo (direcciones/puertos no
//! aparecen en `docs/dsp-plan.md`; `full_scale_counts` y la resolución/cero
//! del encoder SSI son, según sus propios doc-comments en
//! `lamula_ingest`, convenciones sin calibración/documentación real
//! confirmada), así que no se inventa un valor por defecto para ninguno.
//!
//! `drx_nco_fs_hz`/`drx_nco_word_bits` son el mismo tipo de hueco:
//! `lamula_burst::nco_phase_inc_for_freq_offset` (ver su doc-comment) los
//! necesita para convertir la frecuencia filtrada por el lazo de AFC a la
//! palabra de fase del mensaje `Afc`, y ni `DRx↔DSP` ni `DSP↔RCP` exponen la
//! frecuencia de referencia ni la anchura del acumulador de fase del NCO de
//! recepción del DRx — quien despliegue este binario tiene que confirmarlos
//! contra la especificación del hardware DRx, no hay valor por defecto que
//! inventar. `afc_tau_s`/`afc_amp_threshold` son parámetros de instalación
//! del propio lazo (constante de tiempo del filtro de primer orden, umbral
//! de amplitud de burst bajo el cual se congela y se marca BITE — ver
//! `lamula_burst::AfcLoop`), tampoco documentados en ningún contrato.
//!
//! `drx_trigger_fs_hz` es el mismo tipo de hueco otra vez, para un tercer
//! reloj: `crate::ray::trigger_us_to_cycles` (issue #1 ítem 5) lo necesita
//! para convertir `trigger_delay_N`/`trigger_width_N` de microsegundos
//! (`DSP↔RCP` v1.7) a ciclos (`DRx↔DSP::Config`). **No confirmado que sea
//! el mismo reloj que `drx_nco_fs_hz`** (la referencia del NCO de
//! recepción) — son parámetros de instalación distintos hasta que alguien
//! lo confirme contra la especificación real del DRx; ver el doc-comment
//! de `trigger_us_to_cycles`.
//!
//! `tx_if_hz`/`rx_if_hz` (issue #1 ítem 6, mapeo RCP entrada 3) son las
//! frecuencias intermedias de transmisión/recepción de esta instalación —
//! ninguna vive en ningún contrato, mismo motivo que el resto de esta
//! lista. `rx_if_hz` es `Capabilities.rx_if_hz`/`spectrum_frame.
//! center_freq_hz` (`crate::main::capabilities`, `crate::ray::
//! build_spectrum_frame`): antes de esto salía en 0 sin más remedio.
//! `drx_nco_fs_hz`/`drx_nco_word_bits` ahora también se publican tal cual
//! en `Capabilities` (mismos valores, sin campo nuevo aquí) para que el RCP
//! pueda verificarlos contra la especificación del DRx en vez de confiar a
//! ciegas en esta variable de entorno.

use std::env;
use std::fmt;

pub struct ServiceConfig {
    pub drx_addr: String,
    pub rcp_addr: String,
    pub full_scale_counts: i16,
    pub ssi_counts_per_turn: u32,
    pub ssi_zero_offset_deg: f64,
    pub drx_nco_fs_hz: f64,
    pub drx_nco_word_bits: u32,
    pub drx_trigger_fs_hz: f64,
    pub tx_if_hz: f64,
    pub rx_if_hz: f64,
    pub afc_tau_s: f64,
    pub afc_amp_threshold: f64,
    /// Si la fuente de datos de este despliegue es un simulador y no el DRx
    /// real. Se estampa en la cabecera de cada mensaje `up`
    /// (`header_flag::SIMULATED_SOURCE`) para que el RCP no archive dato
    /// simulado como observación. Sin valor por defecto a propósito, igual
    /// que el resto: quien despliega tiene que declararlo.
    pub simulated_source: bool,
}

#[derive(Debug)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ServiceConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(ServiceConfig {
            drx_addr: required("LAMULA_DSP_DRX_ADDR")?,
            rcp_addr: required("LAMULA_DSP_RCP_ADDR")?,
            full_scale_counts: parse_required("LAMULA_DSP_FULL_SCALE_COUNTS")?,
            ssi_counts_per_turn: parse_required("LAMULA_DSP_SSI_COUNTS_PER_TURN")?,
            ssi_zero_offset_deg: parse_required("LAMULA_DSP_SSI_ZERO_OFFSET_DEG")?,
            drx_nco_fs_hz: parse_required("LAMULA_DSP_DRX_NCO_FS_HZ")?,
            drx_nco_word_bits: parse_required("LAMULA_DSP_DRX_NCO_WORD_BITS")?,
            drx_trigger_fs_hz: parse_required("LAMULA_DSP_DRX_TRIGGER_FS_HZ")?,
            tx_if_hz: parse_required("LAMULA_DSP_TX_IF_HZ")?,
            rx_if_hz: parse_required("LAMULA_DSP_RX_IF_HZ")?,
            afc_tau_s: parse_required("LAMULA_DSP_AFC_TAU_S")?,
            afc_amp_threshold: parse_required("LAMULA_DSP_AFC_AMP_THRESHOLD")?,
            simulated_source: parse_bool_required("LAMULA_DSP_SIMULATED_SOURCE")?,
        })
    }
}

fn required(var: &str) -> Result<String, ConfigError> {
    env::var(var).map_err(|_| ConfigError(format!("falta la variable de entorno {var}")))
}

/// `bool` no se parsea con `parse_required`: `str::parse::<bool>` sólo acepta
/// `"true"`/`"false"`, y un fichero de entorno escrito a mano trae `1`, `yes`
/// o `on` con la misma intención. Se aceptan los tres pares, y cualquier otra
/// cosa es error — no se adivina.
fn parse_bool_required(var: &str) -> Result<bool, ConfigError> {
    let raw = required(var)?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(ConfigError(format!(
            "{var}={raw:?} inválido: se esperaba true/false (también 1/0, yes/no, on/off)"
        ))),
    }
}

fn parse_required<T: std::str::FromStr>(var: &str) -> Result<T, ConfigError>
where
    T::Err: fmt::Display,
{
    let raw = required(var)?;
    raw.parse()
        .map_err(|e| ConfigError(format!("{var}={raw:?} inválido: {e}")))
}
