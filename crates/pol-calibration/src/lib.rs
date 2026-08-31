//! Calibración polarimétrica del LAMULA DSP
//! (`docs/algorithms/calibracion-polarimetrica.md`).
//!
//! La página es explícita en que **el DSP aplica la corrección
//! (`zdr_offset_db`, `phidp_offset_deg`, ya resuelto en
//! [`lamula-polarimetry`](../polarimetry)), no la determina en línea** — los
//! procedimientos de campaña viven del lado del operador/RCP. Lo que este
//! crate cubre es la mitad con contenido numérico verificable sin hardware
//! que el oráculo (`tools/oracles/calibracion_polarimetrica.ipynb`) valida:
//! el *procedimiento* de estimación a partir de un dwell ya capturado — la
//! mediana de ZDR en un apuntamiento vertical (birdbath, ZDR verdadero cero
//! por simetría) y la mediana de ΦDP en las primeras celdas de rango, antes
//! de que la fase de propagación acumule nada.
//!
//! Fuera de alcance (ver la página y el oráculo): la inyección de señal de
//! prueba y la comparación con dispersores naturales, que son campañas de
//! banco sin verdad-terreno sintética propia más allá de lo que ya cubre la
//! simetría del birdbath; y el aislamiento de polarización cruzada, que ya
//! tiene su tratamiento numérico en `lamula-polarimetry` (saturación de
//! LDR) y aquí sólo sería un límite de validez configurado, no una cantidad
//! nueva.

mod median;
mod phidp_system;
mod zdr_offset;

pub use phidp_system::phidp_system_offset_deg;
pub use zdr_offset::zdr_offset_from_birdbath_db;
