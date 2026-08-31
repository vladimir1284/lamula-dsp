//! Filtro de clutter GMAP y clasificador de mapa de clutter del LAMULA DSP
//! (`docs/algorithms/gmap-clutter-filtering.md`,
//! `docs/algorithms/mapas-de-clutter.md`).
//!
//! Cubre las partes con oráculo de ambas páginas: el filtro GMAP en el
//! dominio espectral —interpola la banda de clutter con un modelo gaussiano
//! ajustado por mínimos cuadrados a los bins con señal por encima del
//! ruido, degradando a notch cuando el ajuste no es fiable— y el
//! clasificador de persistencia que decide qué celdas entran en el mapa de
//! clutter estático (media de potencia alta y coeficiente de variación
//! temporal bajo, a lo largo de varios barridos). Ambas mitades son
//! funciones puras sobre cantidades ya calculadas por otros algoritmos, sin
//! dependencias de producción, contrastadas numéricamente contra
//! `tools/oracles/gmap_clutter_filtering.ipynb` y
//! `tools/oracles/mapas_de_clutter.ipynb` en
//! `crates/clutter/tests/against_oracle.rs`.
//!
//! Fuera de alcance (ver ambas páginas): la generación/ciclo de vida del
//! mapa en disco, la dependencia con la elevación y el detector dinámico
//! tipo CMD, diferido a Stage 2.

mod filter;
mod map;

pub use filter::{
    gmap_filter, moments_from_spectrum, notch_filter, GmapFilterResult, SpectralMoments,
};
pub use map::{clutter_cell_stats, is_clutter_cell, ClutterCellStats};
