//! Estimador pulse-pair (autocovarianza de retardo 1), Zrnić (1977).
//!
//! `docs/algorithms/pulse-pair-moments.md` §"Cómo funciona" y celda
//! "Estimadores pulse-pair" del oráculo: potencia de `mean(|s|²)` con resta
//! de ruido HS74 (`lamula_noise`), velocidad de la fase de `R(1)`, y ancho
//! espectral invirtiendo el modelo ACF gaussiano cerrado
//! `|R(T)|/R(0) = exp(-8π²σv²T²/λ²)` ya usado en el oráculo del simulador.

use std::f64::consts::PI;

use rustfft::num_complex::Complex64;

use lamula_noise::{noise_floor_estimate, total_power};

/// Estado de la estimación de ancho espectral: los dos casos límite que la
/// página declara explícitamente en vez de dejar en NaN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulsePairFlag {
    /// `S > 0` y `|R(1)|/S < 1`: los tres momentos son válidos.
    Ok,
    /// `S > 0` pero `|R(1)|/S >= 1` (fluctuación de muestra finita, típica de
    /// σv muy pequeño o σv grande con M pequeño): el ancho se recorta a cero
    /// en vez de intentar una raíz de negativo.
    Saturated,
    /// `S <= 0`: celda sin señal detectable tras restar ruido. El ancho no
    /// está definido; la velocidad sí se calcula (la fase de `R(1)` no
    /// depende de la resta de ruido) pero no tiene significado físico sobre
    /// ruido puro.
    Censored,
}

/// Salida del estimador pulse-pair para una celda de rango.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PulsePairEstimate {
    /// `S = max(mean(|s|²) - N̂, 0)`, potencia lineal con ruido restado.
    pub s_linear: f64,
    /// `R̂(0) = mean(|s|²)`, potencia total *sin* restar ruido. La expone
    /// este estimador (no sólo `s_linear`) porque SQI se define sobre ella,
    /// no sobre la señal ya recortada — ver `docs/algorithms/indices-de-calidad.md`
    /// §"SQI" y `lamula_quality::sqi`.
    pub r0_raw: f64,
    /// `|R̂(1)|`, módulo de la autocovarianza a retardo 1: el numerador de
    /// SQI (`lamula_quality::sqi`).
    pub r1_abs: f64,
    /// `N̂`, el suelo de ruido HS74 estimado sobre esta misma ráfaga —
    /// el que de hecho se restó para obtener `s_linear`. Lo expone este
    /// estimador para que SIG (`lamula_quality::sig_db`) se calcule con el
    /// mismo `N̂`, no con un valor distinto.
    pub noise_floor_estimate: f64,
    /// `V = -(λ/4πT)·arg(R̂(1))`, m/s.
    pub velocity_mps: f64,
    /// Ancho espectral, m/s. `None` cuando `flag == Censored`; `Some(0.0)`
    /// cuando `flag == Saturated`.
    pub spectrum_width_mps: Option<f64>,
    pub flag: PulsePairFlag,
}

/// Estima potencia, velocidad y ancho espectral de una ráfaga I/Q por
/// pulse-pair. `y` son las `M` muestras complejas de una celda de rango, ya
/// coherentes (corrección de fase por burst aplicada si el transmisor es un
/// magnetrón — ver `docs/algorithms/burst-fase-afc.md`).
pub fn pulse_pair_moments(y: &[Complex64], wavelength_m: f64, prt_s: f64) -> PulsePairEstimate {
    assert!(y.len() >= 2, "hacen falta al menos dos pulsos");
    assert!(wavelength_m > 0.0, "wavelength_m debe ser positivo");
    assert!(prt_s > 0.0, "prt_s debe ser positivo");

    let r0_raw = total_power(y);
    let n_hat = noise_floor_estimate(y);
    let s_linear = (r0_raw - n_hat).max(0.0);

    let mut r1 = Complex64::new(0.0, 0.0);
    for w in y.windows(2) {
        r1 += w[0] * w[1].conj();
    }
    r1 /= (y.len() - 1) as f64;
    let velocity_mps = -wavelength_m / (4.0 * PI * prt_s) * r1.arg();

    let r1_abs = r1.norm();

    if s_linear <= 0.0 {
        return PulsePairEstimate {
            s_linear,
            r0_raw,
            r1_abs,
            noise_floor_estimate: n_hat,
            velocity_mps,
            spectrum_width_mps: None,
            flag: PulsePairFlag::Censored,
        };
    }

    let ratio = r1_abs / s_linear;
    if ratio >= 1.0 {
        return PulsePairEstimate {
            s_linear,
            r0_raw,
            r1_abs,
            noise_floor_estimate: n_hat,
            velocity_mps,
            spectrum_width_mps: Some(0.0),
            flag: PulsePairFlag::Saturated,
        };
    }

    let spectrum_width_mps =
        wavelength_m / (2.0 * PI * prt_s * 2.0f64.sqrt()) * (-ratio.ln()).sqrt();
    PulsePairEstimate {
        s_linear,
        r0_raw,
        r1_abs,
        noise_floor_estimate: n_hat,
        velocity_mps,
        spectrum_width_mps: Some(spectrum_width_mps),
        flag: PulsePairFlag::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_signal_has_zero_velocity_and_full_ratio() {
        // Señal sin ruido y sin variación de pulso a pulso: R(1) == R(0),
        // ratio == 1 exacto -> saturado, no división por cero ni NaN.
        let y = vec![Complex64::new(1.0, 0.0); 32];
        let est = pulse_pair_moments(&y, 0.10, 1.0e-3);
        assert_eq!(est.velocity_mps, 0.0);
        assert_eq!(est.flag, PulsePairFlag::Saturated);
        assert_eq!(est.spectrum_width_mps, Some(0.0));
    }

    #[test]
    fn pure_zero_signal_is_censored() {
        let y = vec![Complex64::new(0.0, 0.0); 16];
        let est = pulse_pair_moments(&y, 0.10, 1.0e-3);
        assert_eq!(est.s_linear, 0.0);
        assert_eq!(est.flag, PulsePairFlag::Censored);
        assert_eq!(est.spectrum_width_mps, None);
    }
}
