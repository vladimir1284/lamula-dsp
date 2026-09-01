//! Errores del pipeline de ingesta.

use std::fmt;

#[derive(Debug)]
pub enum IngestError {
    Io(std::io::Error),
    BadMagic {
        expected: u32,
        got: u32,
    },
    UnsupportedVersion {
        major: u8,
        minor: u8,
    },
    UnexpectedMsgType(u8),
    Truncated,
    /// La trama no coincide en bins/canales/`channel_mask`/`prf_div` con la
    /// ráfaga que el `RadialAssembler` ya tiene en curso.
    ChannelMismatch,
    /// `seq` repetido: dos tramas con el mismo número de pulso.
    DuplicateSeq,
}

impl fmt::Display for IngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IngestError::Io(e) => write!(f, "error de E/S: {e}"),
            IngestError::BadMagic { expected, got } => {
                write!(
                    f,
                    "magic inválido: esperado {expected:#x}, recibido {got:#x}"
                )
            }
            IngestError::UnsupportedVersion { major, minor } => {
                write!(f, "versión de contrato no soportada: {major}.{minor}")
            }
            IngestError::UnexpectedMsgType(t) => write!(f, "tipo de mensaje inesperado: {t}"),
            IngestError::Truncated => write!(f, "trama truncada o longitud inconsistente"),
            IngestError::ChannelMismatch => write!(
                f,
                "la trama no coincide en bins/canales/prf_div con la ráfaga en curso"
            ),
            IngestError::DuplicateSeq => write!(f, "seq repetido: dos tramas del mismo pulso"),
        }
    }
}

impl std::error::Error for IngestError {}

impl From<std::io::Error> for IngestError {
    fn from(e: std::io::Error) -> Self {
        IngestError::Io(e)
    }
}
