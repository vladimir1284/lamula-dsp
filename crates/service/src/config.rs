//! Parámetros de arranque del binario, todos por variable de entorno: nada
//! de esto tiene un valor documentado en el repo (direcciones/puertos no
//! aparecen en `docs/dsp-plan.md`; `full_scale_counts` y la resolución/cero
//! del encoder SSI son, según sus propios doc-comments en
//! `lamula_ingest`, convenciones sin calibración/documentación real
//! confirmada), así que no se inventa un valor por defecto para ninguno.

use std::env;
use std::fmt;

pub struct ServiceConfig {
    pub drx_addr: String,
    pub rcp_addr: String,
    pub full_scale_counts: i16,
    pub ssi_counts_per_turn: u32,
    pub ssi_zero_offset_deg: f64,
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
