//! Espectro Doppler gaussiano discreto sobre la rejilla de bins de la FFT.

/// Construye la densidad espectral de potencia gaussiana `S(f)` en el orden
/// nativo de bin de la FFT (bin 0 = continua, bins crecientes hasta Nyquist,
/// luego negativos envueltos — el mismo orden que produce/consume
/// `rustfft`), normalizada para que `sum(S) == power_s`.
///
/// El dominio de velocidad es periódico con periodo `2·v_a` (intervalo de
/// Nyquist): se suman réplicas envolventes (`WRAP` a cada lado) para que la
/// cola de la gaussiana no se trunque cuando `sigma_v` se acerca a `v_a`.
pub fn gaussian_doppler_spectrum(
    power_s: f64,
    mean_v: f64,
    sigma_v: f64,
    wavelength_m: f64,
    prt_s: f64,
    m: usize,
) -> Vec<f64> {
    assert!(m > 0, "M debe ser positivo");
    assert!(sigma_v > 0.0, "sigma_v debe ser positivo");

    let v_a = wavelength_m / (4.0 * prt_s);
    // Umbral de signo que coincide con numpy.fft.fftfreq: bins < half son
    // frecuencia positiva, el resto se envuelve a negativa.
    let half = m.div_ceil(2);
    const WRAP: i64 = 3;

    let mut spectrum = vec![0.0f64; m];
    for (k, s) in spectrum.iter_mut().enumerate() {
        let k_signed = if k < half {
            k as i64
        } else {
            k as i64 - m as i64
        };
        let f_k = k_signed as f64 / (m as f64 * prt_s);
        let v_k = f_k * wavelength_m / 2.0;

        let mut acc = 0.0f64;
        for n in -WRAP..=WRAP {
            let dv = v_k - mean_v - (n as f64) * 2.0 * v_a;
            acc += (-0.5 * (dv / sigma_v).powi(2)).exp();
        }
        *s = acc;
    }

    let total: f64 = spectrum.iter().sum();
    for s in spectrum.iter_mut() {
        *s *= power_s / total;
    }
    spectrum
}
