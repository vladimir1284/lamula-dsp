//! Composición de split-cut (`docs/algorithms/procesamiento-de-rango.md`
//! §"Modos de barrido / tipos de corte"): reflectividad del barrido de PRF
//! baja (alcance largo, Nyquist estrecho), velocidad y ancho espectral del
//! barrido de PRF alta (alcance corto, Nyquist amplio). Ambos barridos ya
//! vienen estimados por sus propios algoritmos — ruido y umbrales para la
//! reflectividad, pulse-pair para la velocidad —; esta página sólo decide qué
//! barrido aporta qué campo al radial compuesto.

/// Momentos de un radial compuesto por split-cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitCutMoments {
    /// Reflectividad (potencia lineal, sin convertir a dBZ), del barrido de
    /// PRF baja.
    pub reflectivity_linear: f64,
    /// Velocidad radial, m/s, del barrido de PRF alta.
    pub velocity_mps: f64,
}

/// Selecciona el campo de cada barrido según el reparto de split-cut. No
/// hace ninguna estimación: `low_prf_reflectivity_linear` y
/// `high_prf_velocity_mps` ya son salidas de otros algoritmos, alineadas en
/// azimut por el pipeline antes de llegar aquí.
pub fn compose_split_cut(
    low_prf_reflectivity_linear: f64,
    high_prf_velocity_mps: f64,
) -> SplitCutMoments {
    SplitCutMoments {
        reflectivity_linear: low_prf_reflectivity_linear,
        velocity_mps: high_prf_velocity_mps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_reflectivity_from_low_prf_and_velocity_from_high_prf() {
        let moments = compose_split_cut(1.0, 20.0);
        assert_eq!(moments.reflectivity_linear, 1.0);
        assert_eq!(moments.velocity_mps, 20.0);
    }
}
