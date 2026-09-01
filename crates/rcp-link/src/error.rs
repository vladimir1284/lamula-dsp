//! Errores del enlace DSP↔RCP.

use std::fmt;

#[derive(Debug)]
pub enum RcpLinkError {
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
    /// La trama llegó truncada, o `payload_len`/el tamaño del cuerpo no
    /// coincide con el tamaño fijo del mensaje.
    Truncated,
    /// Sentinela interno de `tcp::spawn`: todos los `Sender<UpMessage>` se
    /// soltaron (cierre intencional del enlace), no un fallo de socket.
    /// No debería verse fuera de ese módulo.
    LinkClosed,
}

impl fmt::Display for RcpLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RcpLinkError::Io(e) => write!(f, "error de E/S: {e}"),
            RcpLinkError::BadMagic { expected, got } => {
                write!(
                    f,
                    "magic inválido: esperado {expected:#x}, recibido {got:#x}"
                )
            }
            RcpLinkError::UnsupportedVersion { major, minor } => {
                write!(f, "versión de contrato no soportada: {major}.{minor}")
            }
            RcpLinkError::UnexpectedMsgType(t) => write!(f, "tipo de mensaje inesperado: {t}"),
            RcpLinkError::Truncated => write!(f, "trama truncada o longitud inconsistente"),
            RcpLinkError::LinkClosed => write!(f, "enlace cerrado intencionalmente"),
        }
    }
}

impl std::error::Error for RcpLinkError {}

impl From<std::io::Error> for RcpLinkError {
    fn from(e: std::io::Error) -> Self {
        RcpLinkError::Io(e)
    }
}
