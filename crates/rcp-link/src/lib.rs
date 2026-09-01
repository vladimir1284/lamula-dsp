//! Enlace `DSP↔RCP` del LAMULA DSP (`contract/schema/dsp_rcp_v0_1.toml`,
//! `docs/contracts/index.md` §"DSP↔RCP: lo diseñamos aquí"; hito M3,
//! `docs/dsp-plan.md`: la observación volumétrica llega al RCP para
//! archivarse como Level-II y alimentar a ORPG).
//!
//! El DSP es el servidor de este enlace: "the RCP is the sole client"
//! (`docs/dsp-plan.md:262`) — al contrario que `lamula_ingest`, donde el DSP
//! también escucha, pero para el DRx.
//!
//! Este crate cubre sólo el cable y la validación de datos de `config`, no
//! el resto de la lógica de negocio:
//!
//! - [`wire`] — codifica los siete mensajes `up` (`moment_ray`,
//!   `spectrum_frame`, `status`, `bite_event`, `config_ack`,
//!   `selftest_result`, `capabilities`) y decodifica los tres `down`
//!   (`config`, `control`, `selftest_request`).
//! - [`tcp`] — adapter real: acepta la conexión del RCP y sirve el enlace
//!   bidireccional sobre ella.
//! - [`validate`] — comprueba un `config` entrante contra las `capabilities`
//!   vigentes y los invariantes físicos que sí están documentados en este
//!   repositorio (ver el doc-comment del módulo para lo que deliberadamente
//!   NO comprueba, y por qué).
//! - [`session`] — máquina de estados `setup`/`running`/`fault`:
//!   `not_in_setup_phase` y `not_configured`. Función pura sobre su propio
//!   estado interno; ver el doc-comment del módulo para lo que deja fuera
//!   (`drx_link_down`, política de cuándo entrar en `fault`).
//!
//! Fuera de alcance de este crate (trabajo futuro):
//!
//! - Ensamblar un [`wire::MomentBlock`] por radial a partir de la salida de
//!   `crates/moments`, `crates/quality`, `crates/polarimetry`, etc. Este
//!   crate sólo sabe empaquetar lo que ya le dan.
//! - El Status & BITE Manager que decide cuándo emitir cada mensaje `up` y
//!   cuándo llamar a [`session::Session::enter_fault`]. `session` sólo hace
//!   las transiciones mecánicas; la política de cuándo dispararlas no es de
//!   este crate.
//! - Adapter en memoria para pruebas sin red, análogo a
//!   `lamula_ingest::simulator`.
//! - Reconexión automática si el RCP cierra la conexión: `tcp::spawn` sirve
//!   una única conexión, igual que `lamula_ingest::tcp`.

mod error;
pub mod session;
pub mod tcp;
pub mod validate;
pub mod wire;

pub use error::RcpLinkError;
pub use session::Session;
pub use tcp::RcpLink;
pub use validate::validate_config;
pub use wire::{DownMessage, MomentBlock, UpMessage};
