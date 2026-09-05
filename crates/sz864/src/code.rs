//! Construcción del código SZ(8/64) (`docs/algorithms/sz-second-trip-recovery.md`
//! §"Cómo funciona la codificación de fase aleatoria (SZ)").
//!
//! Fórmula tomada de Meymaris, Hubbert & Ellis (2005, AMS), que cita
//! literalmente a Sachidananda & Zrnić (1999): código de modulación
//! `φ_k = 8π·k²/64`, `ψ_0 = 0`, con la relación recursiva
//! `φ_k = ψ_{k-1} − ψ_k`. El paper original está bloqueado en este entorno
//! (mismo 403 de AMS/ResearchGate documentado en `docs/algorithms/roadmap.md`
//! para la varianza de pulse-pair) — es inferencia con cita, no verificación
//! directa contra la fuente primaria. Los dos tests de este módulo son la
//! comprobación cruzada independiente: la periodicidad a 32 pulsos y la
//! dispersión en 8 réplicas equiespaciadas son propiedades verificables de la
//! fórmula en sí, no afirmaciones tomadas de la fuente secundaria sin más —
//! mismo criterio que el oráculo (`tools/oracles/sz_second_trip_recovery.ipynb`).

use std::f64::consts::PI;

/// Período del código, en pulsos.
pub const CODE_PERIOD: usize = 32;

fn wrap_to_pi(x: f64) -> f64 {
    (x + PI).rem_euclid(2.0 * PI) - PI
}

/// Código de fase transmitido `ψ_k` (para decodificar al trip fuerte,
/// multiplicando por `e^{-iψ_k}`, ver [`lamula_burst::correct_phase`]) y
/// modulación resultante sobre el trip adyacente `φ_k = ψ_{k-1} − ψ_k` (para
/// recoherenciar el trip débil tras el notch, multiplicando el residuo por
/// `e^{-iφ_k}`). Ambos de longitud `n_pulses`.
pub fn sz_8_64_phases(n_pulses: usize) -> (Vec<f64>, Vec<f64>) {
    assert!(n_pulses > 0, "hace falta al menos un pulso");

    let phi_raw: Vec<f64> = (0..n_pulses)
        .map(|k| 8.0 * PI * (k as f64) * (k as f64) / 64.0)
        .collect();

    let mut acc = 0.0;
    let mut psi = Vec::with_capacity(n_pulses);
    for &p in &phi_raw {
        acc -= p;
        psi.push(wrap_to_pi(acc));
    }
    let phi = phi_raw.into_iter().map(wrap_to_pi).collect();

    (psi, phi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfft::num_complex::Complex64;
    use rustfft::FftPlanner;

    #[test]
    fn psi_repeats_every_32_pulses() {
        let (psi, _phi) = sz_8_64_phases(2 * CODE_PERIOD);
        for k in 0..CODE_PERIOD {
            let diff = wrap_to_pi(psi[k + CODE_PERIOD] - psi[k]);
            assert!(diff.abs() < 1e-9, "k={} diff={}", k, diff);
        }
    }

    /// Prueba 1 del oráculo: decodificar al trip fuerte dispersa al trip
    /// adyacente en exactamente 8 réplicas espectrales equiespaciadas, no en
    /// un pedestal continuo — Meymaris et al. §2.1.
    #[test]
    fn adjacent_trip_modulation_spreads_into_eight_equal_replicas() {
        let (_psi, phi) = sz_8_64_phases(CODE_PERIOD);
        let mut seq: Vec<Complex64> = phi.iter().map(|&p| Complex64::from_polar(1.0, p)).collect();

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(CODE_PERIOD);
        fft.process(&mut seq);

        let mag: Vec<f64> = seq.iter().map(|c| c.norm()).collect();
        let mag_max = mag.iter().cloned().fold(0.0, f64::max);
        let significant: Vec<usize> = (0..CODE_PERIOD)
            .filter(|&k| mag[k] / mag_max > 0.3)
            .collect();

        assert_eq!(
            significant.len(),
            8,
            "bins significativos: {:?}",
            significant
        );
        for w in significant.windows(2) {
            assert_eq!(w[1] - w[0], CODE_PERIOD / 8);
        }
        let mag_min = significant.iter().map(|&k| mag[k]).fold(f64::MAX, f64::min);
        assert!(
            (mag_max - mag_min) / mag_max < 1e-9,
            "las 8 réplicas deberían tener igual magnitud: max={} min={}",
            mag_max,
            mag_min
        );
    }
}
