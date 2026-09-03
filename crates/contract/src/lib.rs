//! Contratos de cable del LAMULA DSP.
//!
//! Este crate no contiene lógica: sólo incorpora el código generado de los dos
//! contratos para que el resto del DSP tenga una sola definición de cada
//! formato, y para que los tests de `tests/` puedan comprobar la disposición
//! real que produce el compilador.
//!
//! Los dos contratos NO se tratan igual, y la diferencia es de propiedad:
//!
//! * [`dsp_rcp`] lo posee este proyecto. Se genera desde
//!   `contract/schema/dsp_rcp_v0_1.toml` con `tools/gen_contract.py`. Para
//!   cambiarlo se edita el esquema y se regenera.
//! * [`drx_dsp`] lo posee el proyecto LAMULA DRx, que lo congeló en su fase Z0.
//!   Aquí sólo se consume: los ficheros de `contract/vendor/` son copias byte a
//!   byte de su salida, ancladas por hash en `contract/vendor/UPSTREAM.toml`.
//!   Para cambiarlo hay que pedirlo allí.
//!
//! Ninguno de los dos ficheros incluidos se edita a mano.

// Se declaran con `#[path]` y no con `include!` porque los dos ficheros
// generados abren con `#![allow(dead_code)]`, y un atributo interno no es legal
// donde `include!` lo expandiría. Como fichero de módulo sí lo es. Esto importa
// sobre todo para el vendorizado: no se puede tocar sin romper el ancla por
// hash, así que el consumidor es quien tiene que adaptarse.

/// Contrato DSP↔RCP v1.0. Propiedad de este proyecto.
#[path = "../../../contract/generated/dsp_rcp_v0_1.rs"]
pub mod dsp_rcp;

/// Contrato DRx↔DSP v0.1. Vendorizado del proyecto LAMULA DRx.
///
/// El `rustfmt::skip` no es cosmético y no se quita: rustfmt desciende por las
/// declaraciones de módulo, así que sin él un `cargo fmt` reescribe el fichero
/// vendorizado —le sobra un espacio en un comentario— y rompe el ancla por hash
/// de `UPSTREAM.toml`. Con él, rustfmt no entra. La comprobación de hash de
/// `tools/check_vendored_contract.py` queda como segunda barrera.
#[rustfmt::skip]
#[path = "../../../contract/vendor/drx_dsp_v0_1.rs"]
pub mod drx_dsp;
