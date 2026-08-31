//! Filtrado de clutter en muestreo escalonado, Sachidananda & Zrnić (2000).
//!
//! Con muestreo uniforme el clutter cae en 0 Hz y un notch ordinario lo
//! separa. Con `T1, T2, T1, T2, …` no hay una FFT ordinaria detrás de la
//! serie completa. El truco: la ráfaga se descompone en las dos
//! subsecuencias uniformes que la componen —índice par (tiempos
//! `0, Ts, 2·Ts, …`) e índice impar (tiempos `T1, T1+Ts, …`), con
//! `Ts = T1+T2`—, cada una sí uniformemente muestreada, así que el clutter
//! estacionario cae exactamente en 0 Hz de cada una; se filtra ahí y se
//! entrelaza de vuelta. Contrastado numéricamente contra
//! `tools/oracles/staggered_prt_clutter_sz2000.ipynb`.
//!
//! Alcance de Stage 1, declarado en el oráculo: notch puro por
//! subsecuencia, no la reconstrucción gaussiana (GMAP) que
//! `crates/clutter` aplica en muestreo uniforme. Cuando la velocidad
//! verdadera cae dentro de la banda de notch de la subsecuencia, el filtro
//! pierde señal real junto con el clutter — limitación medida y declarada
//! en el oráculo, no oculta.

use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

use lamula_noise::noise_floor_estimate;

/// Salida del filtro SZ2000-notch sobre una ráfaga escalonada.
pub struct StaggeredClutterFilter {
    /// Serie completa filtrada, en el mismo orden temporal de entrada.
    pub filtered: Vec<Complex64>,
    /// Ruido (HS74) de la subsecuencia de índice par, estimado *antes* de
    /// filtrar para no sesgar la resta de ruido con el propio notch.
    pub noise_even: f64,
    /// Igual, subsecuencia de índice impar.
    pub noise_odd: f64,
}

/// Filtra el clutter de una ráfaga escalonada `T1, T2, T1, T2, …` (`x.len()`
/// par, al menos 4 muestras). `half_width_bins` fija el ancho de la banda de
/// notch a cada lado de 0 Hz *dentro de cada subsecuencia* (bins `0` a
/// `half_width_bins` inclusive, y sus simétricos por FFT).
pub fn sz2000_clutter_filter(x: &[Complex64], half_width_bins: usize) -> StaggeredClutterFilter {
    assert!(
        x.len() >= 4 && x.len() % 2 == 0,
        "hacen falta al menos 4 muestras, en número par"
    );

    let even: Vec<Complex64> = x.iter().step_by(2).copied().collect();
    let odd: Vec<Complex64> = x.iter().skip(1).step_by(2).copied().collect();

    let (even_filtered, noise_even) = notch_subsequence(&even, half_width_bins);
    let (odd_filtered, noise_odd) = notch_subsequence(&odd, half_width_bins);

    let mut filtered = vec![Complex64::new(0.0, 0.0); x.len()];
    for (i, &v) in even_filtered.iter().enumerate() {
        filtered[2 * i] = v;
    }
    for (i, &v) in odd_filtered.iter().enumerate() {
        filtered[2 * i + 1] = v;
    }

    StaggeredClutterFilter {
        filtered,
        noise_even,
        noise_odd,
    }
}

/// Potencia lineal recuperada tras el filtro, promedio de las dos
/// subsecuencias con su propio ruido restado (recortado a cero).
pub fn reflectivity_estimate(x: &[Complex64], half_width_bins: usize) -> f64 {
    let out = sz2000_clutter_filter(x, half_width_bins);
    let even: Vec<Complex64> = out.filtered.iter().step_by(2).copied().collect();
    let odd: Vec<Complex64> = out.filtered.iter().skip(1).step_by(2).copied().collect();
    let s_even = (mean_power(&even) - out.noise_even).max(0.0);
    let s_odd = (mean_power(&odd) - out.noise_odd).max(0.0);
    0.5 * (s_even + s_odd)
}

fn mean_power(y: &[Complex64]) -> f64 {
    y.iter().map(|c| c.norm_sqr()).sum::<f64>() / y.len() as f64
}

fn notch_subsequence(sub: &[Complex64], half_width_bins: usize) -> (Vec<Complex64>, f64) {
    let m = sub.len();
    let noise = noise_floor_estimate(sub);

    let mut buf: Vec<Complex64> = sub.to_vec();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(m);
    fft.process(&mut buf);

    let hw = half_width_bins.min(m - 1);
    for bin in buf.iter_mut().take(hw + 1) {
        *bin = Complex64::new(0.0, 0.0);
    }
    if hw > 0 && m > hw {
        for bin in buf[(m - hw)..].iter_mut() {
            *bin = Complex64::new(0.0, 0.0);
        }
    }

    let ifft = planner.plan_fft_inverse(m);
    ifft.process(&mut buf);
    let scale = 1.0 / m as f64;
    for v in buf.iter_mut() {
        *v *= scale;
    }

    (buf, noise)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_signal_is_removed_as_clutter() {
        let x = vec![Complex64::new(3.0, 0.0); 16];
        let out = sz2000_clutter_filter(&x, 1);
        let residual: f64 = out.filtered.iter().map(|c| c.norm_sqr()).sum();
        assert!(
            residual < 1e-18,
            "una señal constante (clutter puro) debe anularse casi por completo, residuo={residual}"
        );
    }
}
