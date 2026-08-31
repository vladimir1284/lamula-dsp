//! ΦDP de sistema: separar el offset de equipo de la fase de propagación.
//!
//! `docs/algorithms/calibracion-polarimetrica.md` §"Cómo funciona" y celda
//! "Prueba 3 — ΦDP de sistema" del oráculo: se estima como la mediana del
//! ΦDP medido en las primeras celdas de rango con eco meteorológico, donde
//! la fase acumulada de propagación es todavía despreciable.

use crate::median::median;

/// Estima ΦDP de sistema (grados) como la mediana de las primeras
/// `first_gates` celdas de `phidp_measured_deg`. El perfil debe empezar en
/// la primera celda con eco meteorológico coherente (censura previa por ρHV
/// bajo ya hecha aguas arriba, dependencia con `lamula-polarimetry`) y
/// `first_gates` debe cubrir sólo el tramo donde la fase de propagación es
/// despreciable frente al offset de sistema.
pub fn phidp_system_offset_deg(phidp_measured_deg: &[f64], first_gates: usize) -> f64 {
    assert!(first_gates > 0, "first_gates debe ser positivo");
    assert!(
        first_gates <= phidp_measured_deg.len(),
        "first_gates no puede exceder la longitud del perfil"
    );
    median(&phidp_measured_deg[..first_gates])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_system_phase_from_flat_prefix() {
        let profile = vec![15.0, 14.8, 15.2, 15.1, 14.9, 30.0, 45.0];
        let est = phidp_system_offset_deg(&profile, 5);
        assert!((est - 15.0).abs() < 0.5);
    }

    #[test]
    #[should_panic(expected = "first_gates no puede exceder")]
    fn panics_when_window_exceeds_profile() {
        phidp_system_offset_deg(&[1.0, 2.0], 5);
    }
}
