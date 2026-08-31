//! ZDR, ρHV y ΦDP a partir de la matriz de covarianza entre canales H y V.
//!
//! `docs/algorithms/polarimetria-covarianzas.md` §"Cómo funciona" y celdas
//! "Modo simultáneo" / "Modo alternante" del oráculo: potencias por canal con
//! resta de ruido HS74 (`lamula_noise`), covarianza cruzada `R_hv` sin restar
//! ruido (independiente entre canales, no sesga el valor esperado — mismo
//! argumento que `R(1)` en pulse-pair), y censura por margen de SNR mínimo
//! por canal.

use rustfft::num_complex::Complex64;

use lamula_noise::{noise_floor_estimate, total_power};

/// Estado de la estimación polarimétrica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolarimetricFlag {
    /// Ambos canales superan el margen de SNR mínimo: los tres momentos son
    /// válidos.
    Ok,
    /// `P_h` o `P_v` no superan `min_snr_lin` veces su propio ruido: sin esta
    /// guarda, ρHV diverge a valores absurdos cuando el denominador se
    /// acerca a cero en vez de censurarse.
    Censored,
}

/// Salida de la estimación polarimétrica para una celda de rango.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarimetricEstimate {
    /// `ZDR = 10·log10(P_h/P_v) − zdr_offset_db`, dB. `NaN` si `Censored`.
    pub zdr_db: f64,
    /// `ρHV = |R_hv| / sqrt(P_h·P_v)`, recortado a 1.0. `NaN` si `Censored`.
    pub rhohv: f64,
    /// `ΦDP = arg(R_hv) − phidp_offset_deg`, envuelto a `(-180, 180]`. `NaN`
    /// si `Censored`.
    pub phidp_deg: f64,
    pub flag: PolarimetricFlag,
}

const CENSORED: PolarimetricEstimate = PolarimetricEstimate {
    zdr_db: f64::NAN,
    rhohv: f64::NAN,
    phidp_deg: f64::NAN,
    flag: PolarimetricFlag::Censored,
};

/// Envuelve un ángulo en grados a `(-180, 180]`.
fn wrap_deg(deg: f64) -> f64 {
    let mut wrapped = deg % 360.0;
    if wrapped <= -180.0 {
        wrapped += 360.0;
    } else if wrapped > 180.0 {
        wrapped -= 360.0;
    }
    wrapped
}

/// Potencia por canal con resta de ruido HS74, `(potencia, ruido)`.
fn channel_power(y: &[Complex64]) -> (f64, f64) {
    let r0 = total_power(y);
    let n_hat = noise_floor_estimate(y);
    ((r0 - n_hat).max(0.0), n_hat)
}

/// `mean(yh[i]·conj(yv[i]))` sobre las `M` muestras.
fn cross_covariance(yh: &[Complex64], yv: &[Complex64]) -> Complex64 {
    let mut sum = Complex64::new(0.0, 0.0);
    for (h, v) in yh.iter().zip(yv.iter()) {
        sum += h * v.conj();
    }
    sum / yh.len() as f64
}

/// Estima ZDR, ρHV y ΦDP en modo simultáneo (STAR): `yh`, `yv` son las `M`
/// muestras complejas simultáneas de una celda de rango, un canal cada una.
/// `min_snr_lin` es el margen de SNR mínimo por canal por debajo del cual la
/// celda se censura (0.05 en el oráculo).
pub fn polarimetric_moments_simultaneous(
    yh: &[Complex64],
    yv: &[Complex64],
    zdr_offset_db: f64,
    phidp_offset_deg: f64,
    min_snr_lin: f64,
) -> PolarimetricEstimate {
    assert_eq!(yh.len(), yv.len(), "yh e yv deben tener la misma longitud");
    assert!(yh.len() >= 2, "hacen falta al menos dos pulsos");

    let (ph, nh) = channel_power(yh);
    let (pv, nv) = channel_power(yv);
    if ph <= 0.0 || pv <= 0.0 || ph < min_snr_lin * nh || pv < min_snr_lin * nv {
        return CENSORED;
    }

    let rhv = cross_covariance(yh, yv);
    let zdr_db = 10.0 * (ph / pv).log10() - zdr_offset_db;
    let rhohv = (rhv.norm() / (ph * pv).sqrt()).min(1.0);
    let phidp_deg = wrap_deg(rhv.arg().to_degrees() - phidp_offset_deg);

    PolarimetricEstimate {
        zdr_db,
        rhohv,
        phidp_deg,
        flag: PolarimetricFlag::Ok,
    }
}

/// Estima ZDR, ρHV y ΦDP en modo alternante (H/V conmutados pulso a pulso):
/// `h`, `v` son las `M` muestras de cada canal, medidas a retardo medio-PRT
/// entre sí (`h[i]` y `v[i]` separadas por `t_step_s`, la mitad del PRT de
/// canal). ρHV se corrige por el factor de decorrelación espectral
/// `|ρ(T)| = exp(-8π²·sigma_v_mps²·t_step_s²/λ²)` (Sachidananda & Zrnić 1989,
/// celda "Modo alternante" del oráculo) — `sigma_v_mps` es el ancho espectral
/// ya estimado para la celda (p.ej. por pulse-pair sobre el canal H a su
/// propio PRT), no se re-estima aquí. ZDR y ΦDP usan la misma fórmula directa
/// que en modo simultáneo — ver limitación de ΦDP en la documentación del
/// crate.
#[allow(clippy::too_many_arguments)]
pub fn polarimetric_moments_alternating(
    h: &[Complex64],
    v: &[Complex64],
    zdr_offset_db: f64,
    phidp_offset_deg: f64,
    sigma_v_mps: f64,
    wavelength_m: f64,
    t_step_s: f64,
    min_snr_lin: f64,
) -> PolarimetricEstimate {
    assert_eq!(h.len(), v.len(), "h y v deben tener la misma longitud");
    assert!(h.len() >= 2, "hacen falta al menos dos pulsos por canal");
    assert!(wavelength_m > 0.0, "wavelength_m debe ser positivo");
    assert!(t_step_s > 0.0, "t_step_s debe ser positivo");

    let (ph, nh) = channel_power(h);
    let (pv, nv) = channel_power(v);
    if ph <= 0.0 || pv <= 0.0 || ph < min_snr_lin * nh || pv < min_snr_lin * nv {
        return CENSORED;
    }

    let rho_t = (-8.0 * std::f64::consts::PI.powi(2) * sigma_v_mps.powi(2) * t_step_s.powi(2)
        / wavelength_m.powi(2))
    .exp();

    let rhv = cross_covariance(h, v);
    let zdr_db = 10.0 * (ph / pv).log10() - zdr_offset_db;
    let rho_measured = rhv.norm() / (ph * pv).sqrt();
    let rhohv = (rho_measured / rho_t).min(1.0);
    let phidp_deg = wrap_deg(rhv.arg().to_degrees() - phidp_offset_deg);

    PolarimetricEstimate {
        zdr_db,
        rhohv,
        phidp_deg,
        flag: PolarimetricFlag::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_channels_give_zero_zdr_and_full_correlation() {
        let y: Vec<Complex64> = (0..32)
            .map(|i| Complex64::from_polar(1.0, i as f64 * 0.2))
            .collect();
        let est = polarimetric_moments_simultaneous(&y, &y, 0.0, 0.0, 0.05);
        assert_eq!(est.flag, PolarimetricFlag::Ok);
        assert!(est.zdr_db.abs() < 1e-9);
        assert!((est.rhohv - 1.0).abs() < 1e-9);
        assert!(est.phidp_deg.abs() < 1e-9);
    }

    #[test]
    fn zero_signal_is_censored() {
        let y = vec![Complex64::new(0.0, 0.0); 16];
        let est = polarimetric_moments_simultaneous(&y, &y, 0.0, 0.0, 0.05);
        assert_eq!(est.flag, PolarimetricFlag::Censored);
        assert!(est.zdr_db.is_nan());
    }

    #[test]
    fn offsets_subtract_from_zdr_and_phidp() {
        let y: Vec<Complex64> = (0..32)
            .map(|i| Complex64::from_polar(1.0, i as f64 * 0.2))
            .collect();
        let est = polarimetric_moments_simultaneous(&y, &y, 1.5, 10.0, 0.05);
        assert!((est.zdr_db - (-1.5)).abs() < 1e-9);
        assert!((est.phidp_deg - (-10.0)).abs() < 1e-9);
    }

    #[test]
    fn wrap_deg_handles_boundary() {
        assert!((wrap_deg(190.0) - (-170.0)).abs() < 1e-9);
        assert!((wrap_deg(-190.0) - 170.0).abs() < 1e-9);
        assert!((wrap_deg(180.0) - 180.0).abs() < 1e-9);
    }
}
