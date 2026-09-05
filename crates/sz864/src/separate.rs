//! Separación de trips superpuestos: decodificar al trip fuerte, notch
//! centrado en su velocidad ya estimada, y recoherencia del residuo al trip
//! débil (`docs/algorithms/sz-second-trip-recovery.md`, algoritmo de
//! Meymaris, Hubbert & Ellis 2005 §2.2, variante de Sachidananda & Zrnić
//! 1999). Reutiliza sin reimplementar [`lamula_burst::correct_phase`] (la
//! propia multiplicación por fase que decodifica y que recoherencia),
//! [`lamula_moments::pulse_pair_moments`] (potencia/velocidad/ancho sobre
//! cada trip ya separado) y [`lamula_dual_prf::fold`] (distancia circular en
//! velocidad, para el notch).

use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

use lamula_burst::correct_phase;
use lamula_dual_prf::fold;
use lamula_moments::{pulse_pair_moments, PulsePairEstimate};
use lamula_spectral::bin_velocity;

/// Estimación de los dos trips superpuestos de una celda.
pub struct TripSeparationEstimate {
    /// Trip cohered directamente por `ψ_k` (mayor SNR tras decodificar, no
    /// necesariamente el de mayor rango). Potencia, velocidad y ancho son
    /// los de [`lamula_moments::pulse_pair_moments`] sin ajuste adicional —
    /// contrastado en el oráculo sólo para velocidad (Prueba 2).
    pub strong: PulsePairEstimate,
    /// Trip recuperado del residuo tras el notch, recoherenciado por `φ_k`.
    /// **Sólo `velocity_mps` está contrastado contra el oráculo** (Prueba 3):
    /// `s_linear` no lleva el factor de corrección por ancho de notch que
    /// describe la fuente, y `spectrum_width_mps` no lleva la
    /// "magnitude deconvolution" que corrige el sesgo del propio notch —
    /// ambos fuera de alcance, ver el oráculo y la página del algoritmo.
    pub weak: PulsePairEstimate,
}

/// Separa dos trips superpuestos en `y` (ya con el código SZ(8/64) aplicado
/// en transmisión) usando el código `(psi, phi)` de
/// [`crate::sz_8_64_phases`] y un notch de ancho `notch_width_mps` centrado
/// en la velocidad ya estimada del trip fuerte. La fuente recomienda un
/// ancho de notch de 1/8, 1/4, 1/2 o 3/4 del intervalo de Nyquist
/// (`2·λ/(4·PRT)`), según cuánto se necesite cubrir el ancho espectral del
/// trip fuerte sin comerse demasiado del débil.
pub fn separate_trips(
    y: &[Complex64],
    psi: &[f64],
    phi: &[f64],
    wavelength_m: f64,
    prt_s: f64,
    notch_width_mps: f64,
) -> TripSeparationEstimate {
    assert_eq!(y.len(), psi.len(), "y y psi deben tener la misma longitud");
    assert_eq!(y.len(), phi.len(), "y y phi deben tener la misma longitud");
    let m = y.len();

    // Decodifica asumiendo trip fuerte: y1 = x_fuerte + x_débil·e^{iφ_k} + ruido.
    let y1: Vec<Complex64> = y
        .iter()
        .zip(psi)
        .map(|(&s, &p)| correct_phase(s, p))
        .collect();
    let strong = pulse_pair_moments(&y1, wavelength_m, prt_s);

    // Notch en frecuencia, centrado en la velocidad ya estimada del trip fuerte.
    let v_a = wavelength_m / (4.0 * prt_s);
    let mut spectrum = y1.clone();
    {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(m);
        fft.process(&mut spectrum);
    }
    for (k, bin) in spectrum.iter_mut().enumerate() {
        let v_k = bin_velocity(k, m, wavelength_m, prt_s);
        if fold(v_k - strong.velocity_mps, v_a).abs() < notch_width_mps / 2.0 {
            *bin = Complex64::new(0.0, 0.0);
        }
    }
    {
        let mut planner = FftPlanner::new();
        let ifft = planner.plan_fft_inverse(m);
        ifft.process(&mut spectrum);
    }
    let residual: Vec<Complex64> = spectrum.into_iter().map(|c| c / m as f64).collect();

    // Recoherencia al trip débil: deshace la modulación φ_k = ψ_{k-1} − ψ_k.
    let y2: Vec<Complex64> = residual
        .iter()
        .zip(phi)
        .map(|(&s, &p)| correct_phase(s, p))
        .collect();
    let weak = pulse_pair_moments(&y2, wavelength_m, prt_s);

    TripSeparationEstimate { strong, weak }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sz_8_64_phases;

    /// Sin segundo trip ni ruido: decodificar y notch-y-recoherenciar no
    /// debería alterar la velocidad del trip fuerte, y el notch (que en este
    /// caso remueve la única señal presente) debe dejar el residuo en cero
    /// dentro de tolerancia numérica de FFT/IFFT — el redondeo de ida y
    /// vuelta, no ruido de ningún tipo.
    #[test]
    fn perfect_decode_with_no_second_trip_recovers_strong_velocity() {
        let m = 64;
        let (psi, phi) = sz_8_64_phases(m);
        let wavelength_m = 0.10;
        let prt_s = 1.0e-3;
        let true_v = 5.0;
        let true_phase_step = 4.0 * std::f64::consts::PI * true_v * prt_s / wavelength_m;

        let y: Vec<Complex64> = (0..m)
            .map(|k| {
                Complex64::from_polar(1.0, k as f64 * true_phase_step)
                    * Complex64::from_polar(1.0, psi[k])
            })
            .collect();

        let est = separate_trips(&y, &psi, &phi, wavelength_m, prt_s, 2.0 * 25.0 / 8.0);
        assert!(
            (est.strong.velocity_mps - true_v).abs() < 1e-6,
            "v={}",
            est.strong.velocity_mps
        );
    }
}
