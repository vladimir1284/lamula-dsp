//! Firma de transmisor: secuencias de fase por pulso (magnetrón aleatorio,
//! deriva de frecuencia con perfil conocido) y la muestra de burst que las
//! porta — cierra el hueco que `crate` doc-comment marcaba como fuera de
//! alcance ("firma de transmisor (magnetrón/coherente + burst)").
//! `docs/algorithms/burst-fase-afc.md` describe el fenómeno;
//! `crates/burst` ya mide/corrige/filtra sobre esto y lo contrasta contra
//! oráculo — este módulo sólo genera la señal de entrada de forma
//! reutilizable, en vez de que cada test la arme a mano (como hacía
//! `crates/service::ray::tests::burst_phase_correction_recovers_velocity_from_magnetron_pulse_to_pulse_phase_noise`
//! antes de que existiera este crate-side helper).

use rand::Rng;
use rand_distr::{Distribution, StandardNormal, Uniform};
use rustfft::num_complex::Complex64;

/// Fase inicial uniforme e independiente por pulso, en `(-π, π]` — el modelo
/// de un magnetrón libre (`docs/algorithms/burst-fase-afc.md` §"Qué
/// resuelve"): cada pulso arranca con fase aleatoria, sin relación con el
/// anterior.
pub fn magnetron_phase_sequence(n_pulses: usize, rng: &mut impl Rng) -> Vec<f64> {
    let dist = Uniform::new(-std::f64::consts::PI, std::f64::consts::PI);
    (0..n_pulses).map(|_| dist.sample(rng)).collect()
}

/// Fase acumulada de un offset de frecuencia que varía con el tiempo,
/// muestreada cada `dt_s` segundos: `φ[n] = 2π·Σ_{k<=n}
/// freq_offset_hz_at(k·dt_s)·dt_s` (integración de Euler explícita,
/// suficiente para perfiles suaves como rampa/escalón/ruido — no hace falta
/// más precisión que la que ya usa
/// `crates/burst/tests/against_oracle.rs::afc_loop_matches_oracle_step_ramp_noise_and_loss_behavior`
/// para la misma clase de escenario). `freq_offset_hz_at` recibe el tiempo en
/// segundos desde la primera muestra.
///
/// Agnóstico de qué eje representa `dt_s`: sirve tanto para modelar deriva
/// **entre pulsos/radiales** (`dt_s` = periodo de actualización del lazo de
/// AFC, para ejercitar `lamula_burst::AfcLoop`) como para modelar la
/// frecuencia **dentro de una sola ventana de burst** (`dt_s` = periodo de
/// muestreo en tiempo rápido, para ejercitar
/// `lamula_burst::burst_freq_estimate` directamente — ver el segundo caso en
/// los tests de este módulo).
pub fn drifting_phase_sequence(
    n_samples: usize,
    dt_s: f64,
    freq_offset_hz_at: impl Fn(f64) -> f64,
) -> Vec<f64> {
    assert!(dt_s > 0.0, "dt_s debe ser positivo");
    let mut phase = 0.0;
    (0..n_samples)
        .map(|n| {
            let t = n as f64 * dt_s;
            phase += 2.0 * std::f64::consts::PI * freq_offset_hz_at(t) * dt_s;
            phase
        })
        .collect()
}

/// Multiplica cada muestra de `channel` (una por pulso, mismo orden que
/// `phases`) por `exp(j·phases[i])` — aplica la misma fase de transmisión
/// que ya lleva la muestra de burst (ver [`generate_burst`]) al canal de eco,
/// tal como saldrían las dos del mismo pulso transmitido
/// (`docs/algorithms/burst-fase-afc.md` §"Corrección de fase", sentido
/// inverso de `lamula_burst::correct_phase`).
///
/// # Panics
/// Si `channel.len() != phases.len()`.
pub fn apply_transmit_phase(channel: &[Complex64], phases: &[f64]) -> Vec<Complex64> {
    assert_eq!(
        channel.len(),
        phases.len(),
        "channel y phases deben tener el mismo número de pulsos"
    );
    channel
        .iter()
        .zip(phases)
        .map(|(&s, &phi)| s * Complex64::from_polar(1.0, phi))
        .collect()
}

/// Genera la muestra de burst de un pulso por posición de `phases`:
/// amplitud constante `amplitude` a esa fase, más ruido térmico aditivo
/// `CN(0, noise_floor)` — mismo modelo de ruido que
/// `crate::generate::generate_cell` (partes real/imaginaria independientes
/// `N(0, noise_floor/2)`), no reexportado de ahí porque esa función es
/// privada al módulo `generate` (conformado espectral vía IFFT, que el burst
/// no necesita: no tiene ancho Doppler, es una sola muestra por pulso a
/// amplitud constante).
pub fn generate_burst(
    phases: &[f64],
    amplitude: f64,
    noise_floor: f64,
    rng: &mut impl Rng,
) -> Vec<Complex64> {
    assert!(amplitude >= 0.0, "amplitude debe ser no negativa");
    assert!(noise_floor >= 0.0, "noise_floor debe ser no negativa");
    let sigma = (noise_floor / 2.0).sqrt();
    phases
        .iter()
        .map(|&phi| {
            let noise = if noise_floor > 0.0 {
                let re: f64 = StandardNormal.sample(rng);
                let im: f64 = StandardNormal.sample(rng);
                Complex64::new(re * sigma, im * sigma)
            } else {
                Complex64::new(0.0, 0.0)
            };
            Complex64::from_polar(amplitude, phi) + noise
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamula_burst::{burst_freq_estimate, burst_phase_estimate, correct_phase};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn magnetron_phase_sequence_stays_in_range_and_is_not_constant() {
        let mut rng = StdRng::seed_from_u64(1);
        let phases = magnetron_phase_sequence(256, &mut rng);
        assert_eq!(phases.len(), 256);
        assert!(phases
            .iter()
            .all(|&p| (-std::f64::consts::PI..=std::f64::consts::PI).contains(&p)));
        assert!(phases.iter().any(|&p| (p - phases[0]).abs() > 1e-6));
    }

    #[test]
    fn apply_transmit_phase_recovers_via_correct_phase() {
        let mut rng = StdRng::seed_from_u64(2);
        let phases = magnetron_phase_sequence(64, &mut rng);
        let coherent: Vec<Complex64> = (0..64)
            .map(|i| Complex64::new(1.0 + i as f64, 0.0))
            .collect();
        let scrambled = apply_transmit_phase(&coherent, &phases);
        let recovered: Vec<Complex64> = scrambled
            .iter()
            .zip(&phases)
            .map(|(&s, &phi)| correct_phase(s, phi))
            .collect();
        for (a, b) in coherent.iter().zip(&recovered) {
            assert!((a - b).norm() < 1e-9);
        }
    }

    #[test]
    fn drifting_phase_sequence_matches_constant_offset_analytically() {
        // Offset constante `f0`: fase acumulada en el pulso `n` es
        // `2*pi*f0*(n+1)*prt_s` (suma de una constante `n+1` veces).
        const F0_HZ: f64 = 50.0;
        const PRT_S: f64 = 1.0e-3;
        let phases = drifting_phase_sequence(10, PRT_S, |_t| F0_HZ);
        for (n, &phi) in phases.iter().enumerate() {
            let expected = 2.0 * std::f64::consts::PI * F0_HZ * (n + 1) as f64 * PRT_S;
            assert!(
                (phi - expected).abs() < 1e-9,
                "pulso {n}: {phi} vs {expected}"
            );
        }
    }

    #[test]
    fn generate_burst_amplitude_and_phase_recoverable_without_noise() {
        let phases = vec![0.3, -1.2, 2.7];
        let mut rng = StdRng::seed_from_u64(3);
        let burst = generate_burst(&phases, 5.0, 0.0, &mut rng);
        for (b, &phi) in burst.iter().zip(&phases) {
            assert!((b.norm() - 5.0).abs() < 1e-9);
            assert!((b.arg() - phi).abs() < 1e-9);
        }
    }

    #[test]
    fn generate_burst_feeds_burst_freq_estimate_close_to_injected_drift() {
        // Sanity end-to-end: no repite el contraste de oráculo de
        // `crates/burst` (ya hecho ahí), sólo confirma que el generador de
        // este crate produce una señal que ese estimador recupera.
        const F0_HZ: f64 = 1200.0;
        const DT_FAST_S: f64 = 1.0e-6;
        let phases = drifting_phase_sequence(64, DT_FAST_S, |_t| F0_HZ);
        let mut rng = StdRng::seed_from_u64(4);
        let burst = generate_burst(&phases, 10.0, 0.0, &mut rng);
        let estimated = burst_freq_estimate(&burst, DT_FAST_S);
        assert!(
            (estimated - F0_HZ).abs() < 1.0,
            "estimado={estimated}, inyectado={F0_HZ}"
        );
    }

    #[test]
    fn burst_phase_estimate_matches_injected_phase_without_noise() {
        // 8 muestras de una misma ventana de burst, todas a la misma fase:
        // ejercita el promedio coherente de `burst_phase_estimate`, no sólo
        // el caso trivial de una muestra.
        const PHI: f64 = 1.234;
        let phases = vec![PHI; 8];
        let mut rng = StdRng::seed_from_u64(6);
        let burst = generate_burst(&phases, 8.0, 0.0, &mut rng);
        let estimated = burst_phase_estimate(&burst);
        assert!((estimated - PHI).abs() < 1e-9);
    }
}
