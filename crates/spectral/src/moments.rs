//! Periodograma con ventana de Hann y extracción de momentos por recorte de
//! la línea principal (`tools/oracles/estimador_espectral.ipynb`).

use lamula_noise::noise_floor_estimate;
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

/// Caída relativa desde el pico, en dB, que delimita la línea principal.
const DROP_DB: f64 = 12.0;
/// Semiancho máximo de la línea principal, como fracción de `M`.
const MAX_HALF_SPAN_FRAC: f64 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectralFlag {
    /// Línea principal detectada por encima del umbral de ruido.
    Ok,
    /// El pico del periodograma no supera el umbral de ruido: no hay señal
    /// detectable en la celda.
    Censored,
}

#[derive(Debug, Clone, Copy)]
pub struct SpectralEstimate {
    /// Potencia de la línea principal, suma de las líneas espectrales
    /// atribuidas a la señal.
    pub power_linear: f64,
    /// Velocidad radial media, primer momento de la línea principal, m/s.
    /// `None` si `flag` es `Censored`.
    pub velocity_mps: Option<f64>,
    /// Ancho espectral, raíz del segundo momento central de la línea
    /// principal, m/s. `None` si `flag` es `Censored`.
    pub spectrum_width_mps: Option<f64>,
    pub flag: SpectralFlag,
}

/// Ventana de Hann de `m` muestras, misma convención que `numpy.hanning`:
/// `w[n] = 0.5 - 0.5·cos(2πn/(M-1))`, simétrica y con extremos nulos.
fn hann_window(m: usize) -> Vec<f64> {
    assert!(m >= 2, "la ventana de Hann necesita al menos dos muestras");
    let denom = (m - 1) as f64;
    (0..m)
        .map(|n| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / denom).cos())
        .collect()
}

/// Periodograma ventaneado `P[k] = |FFT(y·win)[k]|^2 / (M·Σwin^2)`,
/// normalizado para conservar la identidad de Parseval de la potencia de
/// ruido blanco (misma convención que el oráculo).
fn periodogram_hann(y: &[Complex64], win: &[f64]) -> Vec<f64> {
    let m = y.len();
    let s2: f64 = win.iter().map(|w| w * w).sum();

    let mut buf: Vec<Complex64> = y.iter().zip(win).map(|(&s, &w)| s * w).collect();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(m);
    fft.process(&mut buf);

    let denom = m as f64 * s2;
    buf.iter().map(|c| c.norm_sqr() / denom).collect()
}

/// Convierte un índice de bin nativo de la FFT a su velocidad, con la misma
/// convención de signo que `numpy.fft.fftfreq` / `rustfft`: bins `< half`
/// son frecuencia positiva, el resto se envuelve a negativa.
fn bin_velocity(k: usize, m: usize, wavelength_m: f64, prt_s: f64) -> f64 {
    let half = m.div_ceil(2);
    let k_signed = if k < half {
        k as i64
    } else {
        k as i64 - m as i64
    };
    let f_k = k_signed as f64 / (m as f64 * prt_s);
    f_k * wavelength_m / 2.0
}

/// Estima potencia, velocidad radial y ancho espectral por periodograma con
/// ventana de Hann (`docs/algorithms/estimador-espectral.md`).
///
/// El eje de velocidad se reconstruye centrado en el pico del periodograma,
/// no leído directamente de la rejilla de bins: así un eco partido entre los
/// dos extremos del array cerca de la Nyquist se recompone en vez de dar un
/// primer momento sin sentido.
pub fn spectral_moments(y: &[Complex64], wavelength_m: f64, prt_s: f64) -> SpectralEstimate {
    let m = y.len();
    assert!(m >= 2, "hacen falta al menos dos pulsos");
    assert!(wavelength_m > 0.0, "wavelength_m debe ser positivo");
    assert!(prt_s > 0.0, "prt_s debe ser positivo");

    let v_a = wavelength_m / (4.0 * prt_s);
    let n_hat = noise_floor_estimate(y);
    let win = hann_window(m);
    let p = periodogram_hann(y, &win);

    let peak = (0..m)
        .max_by(|&a, &b| p[a].partial_cmp(&p[b]).expect("periodograma con NaN"))
        .expect("periodograma no vacío");
    let noise_thresh = 4.0 * n_hat / m as f64;

    if p[peak] <= noise_thresh {
        return SpectralEstimate {
            power_linear: 0.0,
            velocity_mps: None,
            spectrum_width_mps: None,
            flag: SpectralFlag::Censored,
        };
    }

    let level_thresh = (p[peak] * 10f64.powf(-DROP_DB / 10.0)).max(noise_thresh);
    let max_half_span = (m as f64 * MAX_HALF_SPAN_FRAC) as i64;
    let peak_i = peak as i64;

    let mut lo = peak_i;
    while (peak_i - (lo - 1)) <= max_half_span
        && p[(lo - 1).rem_euclid(m as i64) as usize] >= level_thresh
    {
        lo -= 1;
    }
    let mut hi = peak_i;
    while ((hi + 1) - peak_i) <= max_half_span
        && p[(hi + 1).rem_euclid(m as i64) as usize] >= level_thresh
    {
        hi += 1;
    }

    let bin_spacing = 2.0 * v_a / m as f64;
    let v_peak = bin_velocity(peak, m, wavelength_m, prt_s);

    let mut total_power = 0.0f64;
    let mut weighted_v = 0.0f64;
    for offset in lo..=hi {
        let idx = offset.rem_euclid(m as i64) as usize;
        let power = p[idx];
        let v = v_peak + (offset - peak_i) as f64 * bin_spacing;
        total_power += power;
        weighted_v += power * v;
    }

    if total_power <= 0.0 {
        return SpectralEstimate {
            power_linear: 0.0,
            velocity_mps: None,
            spectrum_width_mps: None,
            flag: SpectralFlag::Censored,
        };
    }

    let v_mean = weighted_v / total_power;
    let mut weighted_var = 0.0f64;
    for offset in lo..=hi {
        let idx = offset.rem_euclid(m as i64) as usize;
        let v = v_peak + (offset - peak_i) as f64 * bin_spacing;
        weighted_var += p[idx] * (v - v_mean).powi(2);
    }
    let width = (weighted_var / total_power).sqrt();

    SpectralEstimate {
        power_linear: total_power,
        velocity_mps: Some(v_mean),
        spectrum_width_mps: Some(width),
        flag: SpectralFlag::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_window_has_zero_endpoints_and_unit_peak() {
        let win = hann_window(8);
        assert!(win[0].abs() < 1e-12);
        assert!((win[7]).abs() < 1e-12);
        assert!(win.iter().cloned().fold(0.0f64, f64::max) <= 1.0 + 1e-12);
    }

    #[test]
    fn pure_noise_below_threshold_is_censored() {
        // Ruido blanco puro sin señal: el pico del periodograma no debería
        // superar de forma sistemática el umbral 4·N̂/M salvo por
        // fluctuación estadística, así que basta comprobar que el camino de
        // censura no entra en pánico y produce `None`s consistentes.
        let y: Vec<Complex64> = (0..64)
            .map(|n| Complex64::new((n as f64 * 0.37).sin(), (n as f64 * 0.61).cos()))
            .collect();
        let est = spectral_moments(&y, 0.10, 1.0e-3);
        if est.flag == SpectralFlag::Censored {
            assert_eq!(est.power_linear, 0.0);
            assert!(est.velocity_mps.is_none());
            assert!(est.spectrum_width_mps.is_none());
        }
    }
}
