//! Asignación de celda de rango (`docs/algorithms/procesamiento-de-rango.md`
//! §"Asignación de rango" y §"Resolución y promediado en rango").

/// Índice de gate y rango de su centro para un blanco a `range_m`, dada la
/// geometría de barrido: `start_range_m` es el retardo de sistema ya
/// convertido a rango, `fine_spacing_m` es la resolución física que da el
/// ancho de pulso (`pulse_width_idx`), y `cell_mode_k` el factor de
/// promediado de celda gruesa (`cell_mode` del contrato `DRx↔DSP`: 1 = sin
/// promediar). `None` si `range_m` cae antes de `start_range_m` o después del
/// último gate — fuera de alcance, no un error.
pub fn assign_range_gate(
    range_m: f64,
    start_range_m: f64,
    fine_spacing_m: f64,
    cell_mode_k: u32,
    n_gates: u32,
) -> Option<(u32, f64)> {
    assert!(fine_spacing_m > 0.0, "fine_spacing_m debe ser positivo");
    assert!(cell_mode_k > 0, "cell_mode_k debe ser positivo");

    let coarse_spacing_m = fine_spacing_m * cell_mode_k as f64;
    let offset = range_m - start_range_m;
    if offset < 0.0 {
        return None;
    }
    let idx = (offset / coarse_spacing_m).floor();
    if idx >= n_gates as f64 {
        return None;
    }
    let idx = idx as u32;
    let center = start_range_m + (idx as f64 + 0.5) * coarse_spacing_m;
    Some((idx, center))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_before_start_range() {
        assert_eq!(assign_range_gate(240.0, 250.0, 125.0, 1, 400), None);
    }

    #[test]
    fn rejects_past_last_gate() {
        assert_eq!(
            assign_range_gate(250.0 + 125.0 * 400.0 + 10.0, 250.0, 125.0, 1, 400),
            None
        );
    }

    #[test]
    fn assigns_gate_center() {
        let (idx, center) = assign_range_gate(250.0 + 62.5, 250.0, 125.0, 1, 400).unwrap();
        assert_eq!(idx, 0);
        assert!((center - 312.5).abs() < 1e-9);
    }

    #[test]
    fn coarse_cell_mode_widens_spacing() {
        // Con cell_mode_k=4, el segundo gate cubre [250+500, 250+1000).
        let (idx, center) = assign_range_gate(250.0 + 600.0, 250.0, 125.0, 4, 100).unwrap();
        assert_eq!(idx, 1);
        assert!((center - 1000.0).abs() < 1e-9);
    }
}
