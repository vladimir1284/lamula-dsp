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
    pub afc_tau_s: f64,
    pub afc_amp_threshold: f64,
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
            afc_tau_s: parse_required("LAMULA_DSP_AFC_TAU_S")?,
            afc_amp_threshold: parse_required("LAMULA_DSP_AFC_AMP_THRESHOLD")?,
        })
    }
}

fn required(var: &str) -> Result<String, ConfigError> {
    env::var(var).map_err(|_| ConfigError(format!("falta la variable de entorno {var}")))
}

fn parse_required<T: std::str::FromStr>(var: &str) -> Result<T, ConfigError>
where
    T::Err: fmt::Display,
{
    let raw = required(var)?;
    raw.parse()
        .map_err(|e| ConfigError(format!("{var}={raw:?} inválido: {e}")))
}
