//! Offset de ZDR por apuntamiento vertical (birdbath).
//!
//! `docs/algorithms/calibracion-polarimetrica.md` §"Cómo funciona" y celda
//! "Prueba 2 — birdbath" del oráculo: en un dwell a 90° de elevación, en
//! lluvia, el ZDR físico es cero por simetría — las gotas vistas desde abajo
//! tienen la misma sección eficaz en ambas polarizaciones —, así que
//! cualquier ZDR medido en ese dwell es, por construcción, el offset del
//! sistema.

use crate::median::median;

/// Estima el offset de ZDR (dB) a partir de las mediciones de ZDR de un
/// dwell de apuntamiento vertical. `zdr_measurements_db` deben venir ya
/// filtradas de celdas censuradas (`NaN`) por quien llama — este
/// procedimiento no distingue censura de dato válido.
pub fn zdr_offset_from_birdbath_db(zdr_measurements_db: &[f64]) -> f64 {
    assert!(
        !zdr_measurements_db.is_empty(),
        "hace falta al menos una celda del dwell"
    );
    median(zdr_measurements_db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_constant_offset() {
        let measurements = vec![0.6, 0.55, 0.62, 0.58, 0.61];
        let offset = zdr_offset_from_birdbath_db(&measurements);
        assert!((offset - 0.6).abs() < 0.05);
    }

    #[test]
    fn robust_to_one_outlier() {
        let mut measurements = vec![0.6; 20];
        measurements.push(50.0); // celda contaminada
        let offset = zdr_offset_from_birdbath_db(&measurements);
        assert!((offset - 0.6).abs() < 1e-9);
    }
}
