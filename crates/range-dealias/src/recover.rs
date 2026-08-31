//! Recuperación del primer trip por fase aleatoria (magnetrón).
//!
//! `docs/algorithms/dealiasing-de-rango.md` §"Cómo funciona" y celda
//! "Prueba 2" del oráculo: corregir la serie con la fase de burst del primer
//! trip deja su componente coherente; la del segundo trip —transmitida en un
//! pulso anterior con otra fase aleatoria independiente— queda con fase
//! residual uniforme y se blanquea en todo el espectro, indistinguible del
//! ruido térmico para Hildebrand & Sekhon. Estimar potencia y velocidad
//! sobre la serie corregida recupera entonces el primer trip filtrando ese
//! pedestal — reutiliza sin reimplementar
//! [`lamula_burst::correct_phase`] y [`lamula_moments::pulse_pair_moments`].

use rustfft::num_complex::Complex64;

use lamula_burst::correct_phase;
use lamula_moments::{pulse_pair_moments, PulsePairEstimate};

/// Corrige `y` (mezcla de primer y segundo trip más ruido) con la fase de
/// burst del primer trip, `phi1_rad` (una por pulso), y estima potencia,
/// velocidad y ancho espectral del primer trip sobre la serie corregida. La
/// calidad de la recuperación degrada con la razón de potencias entre trips
/// — ver la curva de aceptación en el oráculo, no un margen único.
pub fn recover_trip1(
    y: &[Complex64],
    phi1_rad: &[f64],
    wavelength_m: f64,
    prt_s: f64,
) -> PulsePairEstimate {
    assert_eq!(
        y.len(),
        phi1_rad.len(),
        "y y phi1_rad deben tener la misma longitud"
    );

    let y_corrected: Vec<Complex64> = y
        .iter()
        .zip(phi1_rad.iter())
        .map(|(&sample, &phi)| correct_phase(sample, phi))
        .collect();

    pulse_pair_moments(&y_corrected, wavelength_m, prt_s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_phase_correction_with_no_second_trip_recovers_signal() {
        // Sin segundo trip: corregir con la fase verdadera debe dar la
        // misma serie coherente que sin fase aleatoria en absoluto.
        let phi1_rad = vec![0.7, -1.2, 2.0, 0.1];
        let y: Vec<Complex64> = phi1_rad
            .iter()
            .map(|&phi| Complex64::from_polar(1.0, phi))
            .collect();
        let est = recover_trip1(&y, &phi1_rad, 0.10, 1.0e-3);
        assert!(est.velocity_mps.abs() < 1e-9, "v={}", est.velocity_mps);
    }
}
