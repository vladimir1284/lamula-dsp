//! Detección de interferencia de banda estrecha (RFI) del LAMULA DSP
//! (`docs/algorithms/rfi-filtrado.md`).
//!
//! Detecta líneas espectrales de RFI por combinación de dos criterios: exceso
//! sobre la mediana del espectro (robusta frente a la propia interferencia,
//! a diferencia de la media) y anchura angosta (el lóbulo principal de la
//! ventana, no la anchura Doppler de un eco meteorológico real, que ocupa
//! más bins cuanto mayor `M`). Un pico meteorológico fuerte sin RFI puede
//! superar la mediana igual que un tono de interferencia; lo que lo
//! descarta es la anchura, no la altura.
//!
//! La interpolación reutiliza el mismo mecanismo de ajuste gaussiano que
//! `lamula_clutter::gmap_filter` usa para el hueco del clutter —la propia
//! página lo señala como "exactamente el mismo mecanismo de relleno"— así
//! que este crate no lo reimplementa: sólo aporta la máscara a rellenar, y
//! quien orqueste el pipeline llama a `gmap_filter` con esa máscara. Sin
//! dependencia de producción de `lamula-clutter`; el contraste contra el
//! oráculo la usa como dependencia de desarrollo para verificar la
//! composición completa. Contrastado numéricamente contra
//! `tools/oracles/rfi_filtrado.ipynb` en `crates/rfi/tests/against_oracle.rs`.
//!
//! Fuera de alcance de este crate (ver la página): la contabilidad conjunta
//! de CCOR entre RFI y clutter, el discriminante polarimétrico por ρHV y la
//! detección por radial completo.

mod detect;

pub use detect::{detect_rfi_mask, spike_width, DEFAULT_RFI_MEDIAN_DB, DEFAULT_RFI_WIDTH_MAX_BINS};
