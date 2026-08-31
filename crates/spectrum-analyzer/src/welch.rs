//! Periodograma de Welch: promedio en potencia lineal, normalización de
//! ganancia coherente.
//!
//! `docs/algorithms/analizador-espectro-fi.md` §"Cómo funciona" y celda
//! `welch_trace_dbm` del oráculo.

use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

/// Traza de Welch en dBm a partir de `captures` (todas del mismo largo que
/// `win`): ventana, FFT, potencia normalizada por ganancia coherente
/// `(Σw)²`, promedio en lineal sobre las capturas, conversión a dB y
/// desplazamiento por `ref_level_dbm_offset` (ganancia de receptor +
/// calibración). El suelo de ruido en esta traza aparece escalado por el
/// ENBW de la ventana ([`crate::enbw_bins`]) — corregir esa lectura es
/// responsabilidad de quien la interpreta, no de esta función, que sólo
/// produce la traza normalizada para leer picos correctamente.
pub fn welch_trace_dbm(
    captures: &[Vec<Complex64>],
    win: &[f64],
    ref_level_dbm_offset: f64,
) -> Vec<f64> {
    assert!(!captures.is_empty(), "hace falta al menos una captura");
    let m = win.len();
    assert!(m > 0, "la ventana no puede estar vacía");
    assert!(
        captures.iter().all(|c| c.len() == m),
        "todas las capturas deben tener la misma longitud que la ventana"
    );

    let sum_w: f64 = win.iter().sum();
    let sw2 = sum_w * sum_w;

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(m);

    let mut avg_power = vec![0.0f64; m];
    for capture in captures {
        let mut buf: Vec<Complex64> = capture
            .iter()
            .zip(win.iter())
            .map(|(&x, &w)| x * w)
            .collect();
        fft.process(&mut buf);
        for (acc, x) in avg_power.iter_mut().zip(buf.iter()) {
            *acc += x.norm_sqr() / sw2;
        }
    }
    let n = captures.len() as f64;
    avg_power
        .into_iter()
        .map(|p| 10.0 * (p / n).max(1e-300).log10() + ref_level_dbm_offset)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::hann_window;

    #[test]
    fn pure_tone_peak_at_correct_bin_and_level() {
        let m = 64;
        let win = hann_window(m);
        let bin_idx = 10;
        let power_lin: f64 = 2.0;
        let capture: Vec<Complex64> = (0..m)
            .map(|n| {
                Complex64::from_polar(
                    power_lin.sqrt(),
                    2.0 * std::f64::consts::PI * bin_idx as f64 * n as f64 / m as f64,
                )
            })
            .collect();
        let trace = welch_trace_dbm(&[capture], &win, 0.0);
        let (peak_bin, &peak_val) = trace
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        assert_eq!(peak_bin, bin_idx);
        let expected_dbm = 10.0 * power_lin.log10();
        assert!(
            (peak_val - expected_dbm).abs() < 0.01,
            "peak_val={peak_val}"
        );
    }
}
