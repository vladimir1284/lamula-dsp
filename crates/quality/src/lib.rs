//! Índices de calidad del LAMULA DSP (`docs/algorithms/indices-de-calidad.md`):
//! SQI, CCOR y SIG.
//!
//! Los tres son funciones puras de cantidades que otros algoritmos del
//! pipeline ya calculan (autocovarianza a retardo 0/1 del
//! [pulse-pair](../moments), potencia antes/después del filtro de clutter,
//! potencia de ruido de la [cadena de ruido](../noise)); este crate no
//! recorre ninguna serie temporal por sí mismo.

mod ccor;
mod sig;
mod sqi;

pub use ccor::ccor_db;
pub use sig::sig_db;
pub use sqi::sqi;
