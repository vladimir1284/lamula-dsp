//! Corrección de atenuación Z-PHI del LAMULA DSP
//! (`docs/algorithms/atenuacion-zphi.md`).
//!
//! Testud, Bouar, Obligis & Ali-Mehenni (2000): sobre un tramo contiguo de
//! lluvia `r1..r2` con reflectividad ya calibrada (ecuación del radar,
//! `lamula_calibration::power_to_dbz`, y filtro de clutter si aplica —
//! `crates/service::ray`, campo CZ) y con la fase diferencial total medida en
//! los dos extremos del tramo (`delta_phidp_deg`), resuelve el perfil de
//! atenuación específica A(r) [dB/km] de forma cerrada, **sin necesitar el
//! coeficiente α de la relación Z-A** (`A_zA = α·Z^β`) — sólo el exponente
//! `β` (que gobierna cómo se reparte la atenuación a lo largo del tramo según
//! la forma del perfil de Z) y el coeficiente de acoplamiento
//! atenuación-fase `a_coef` [dB/grado] (que fija la magnitud total, vía
//! `A ≈ a_coef·KDP`). Ninguno de los dos es un parámetro del contrato
//! `DSP↔RCP` v0.1 — ver el doc-comment de las constantes en
//! `crates/service::ray` que los fija, mismo tipo de hueco que
//! `KDP_WINDOW_GATES`.
//!
//! **Fórmula reconstruida de memoria de la literatura (Testud et al. 2000;
//! Bringi & Chandrasekar 2001, cap. 7) y verificada aquí contra su propia
//! identidad algebraica de autoconsistencia
//! (`self_consistency_two_way_pia_matches_a_coef_times_delta_phidp`), no
//! contra el texto original del paper — no hay acceso a él en este entorno.
//! Los coeficientes por defecto (`β = 0.64884` fijo, `a_coef` por banda) son
//! los que documenta Gu et al. (2011) y usa Py-ART
//! (`pyart.correct.calculate_attenuation_zphi`); se recomienda contrastar la
//! fórmula completa contra esa implementación antes de tratar este crate
//! como validado externamente — el oráculo (`tools/oracles/
//! atenuacion_zphi.ipynb`) lo señala igual.**
//!
//! Precondición de ambas funciones (responsabilidad de quien llama, mismo
//! criterio que `lamula_calibration::power_to_dbz`): `z_dbz` es un tramo YA
//! censurado — sin `NaN`, con eco detectable en las dos funciones —
//! `delta_phidp_deg` ya no-negativo no es precondición: un valor negativo
//! (ruido de fase en tramos sin atenuación real) se trata como "sin
//! atenuación medible" en vez de propagar una corrección de signo
//! equivocado, coherente con el criterio de censura del resto del pipeline
//! ("censura, no corrige" ante lo que no se puede medir con confianza).

/// Perfil de atenuación específica A(r) [dB/km], Testud et al. (2000).
///
/// `z_dbz` es la reflectividad ya calibrada y filtrada de clutter (CZ antes
/// de esta corrección), a lo largo del tramo contiguo `r1..r2` sobre el que
/// se midió `delta_phidp_deg = ΦDP(r2) - ΦDP(r1)` (grados, ya desdoblado —
/// `lamula_kdp::unwrap_deg`). `gate_spacing_km` es el paso de rango entre
/// celdas consecutivas de `z_dbz`.
///
/// Entero desarrollo en el doc-comment del módulo. La identidad de
/// autoconsistencia del método —`2·∫A(r)dr` a lo largo de TODO el tramo
/// coincide exactamente con `a_coef·delta_phidp_deg`, para cualquier forma
/// del perfil de Z— es lo único que este crate puede verificar sin el paper
/// original; lo comprueba el test `self_consistency_...` de este módulo y la
/// prueba homónima del oráculo.
pub fn zphi_specific_attenuation(
    z_dbz: &[f64],
    gate_spacing_km: f64,
    beta: f64,
    a_coef_db_per_deg: f64,
    delta_phidp_deg: f64,
) -> Vec<f64> {
    assert!(
        z_dbz.len() >= 2,
        "hace falta al menos un intervalo (2 celdas) para integrar"
    );
    assert!(gate_spacing_km > 0.0, "gate_spacing_km debe ser positivo");
    assert!(beta > 0.0, "beta debe ser positivo");

    let n = z_dbz.len();
    let z_beta: Vec<f64> = z_dbz
        .iter()
        .map(|&dbz| {
            let z_linear = 10f64.powf(dbz / 10.0);
            assert!(z_linear > 0.0, "z_dbz debe ser finito (tramo sin censurar)");
            z_linear.powf(beta)
        })
        .collect();

    // Integral prefijo S(r) = 0.46*beta*∫_{r1}^r Z(s)^beta ds (regla del
    // trapecio), S[0] = 0 por construcción.
    let mut prefix = vec![0.0; n];
    for i in 1..n {
        prefix[i] =
            prefix[i - 1] + 0.46 * beta * 0.5 * (z_beta[i - 1] + z_beta[i]) * gate_spacing_km;
    }
    let i_total = prefix[n - 1];
    assert!(
        i_total > 0.0,
        "el tramo no tiene señal detectable (Z=0 en todas las celdas); sin eso el método no está definido"
    );

    // ΔΦDP negativo (ruido de fase en tramo sin atenuación real): se censura
    // a "sin atenuación medible" en vez de aplicar una corrección con el
    // signo equivocado -- ver el doc-comment del módulo.
    let delta_phidp_deg = delta_phidp_deg.max(0.0);
    let c = 10f64.powf(0.1 * beta * a_coef_db_per_deg * delta_phidp_deg) - 1.0;

    (0..n)
        .map(|i| {
            let tail = i_total - prefix[i];
            let denom = i_total + c * tail;
            z_beta[i] * c / denom
        })
        .collect()
}

/// Reflectividad corregida de atenuación [dBZ]: `z_dbz` más el camino
/// bidireccional acumulado de [`zphi_specific_attenuation`] hasta cada
/// celda. Ver el doc-comment del módulo para las precondiciones.
pub fn zphi_correct_dbz(
    z_dbz: &[f64],
    gate_spacing_km: f64,
    beta: f64,
    a_coef_db_per_deg: f64,
    delta_phidp_deg: f64,
) -> Vec<f64> {
    let a = zphi_specific_attenuation(
        z_dbz,
        gate_spacing_km,
        beta,
        a_coef_db_per_deg,
        delta_phidp_deg,
    );
    let n = a.len();
    let mut two_way_correction = vec![0.0; n];
    for i in 1..n {
        two_way_correction[i] = two_way_correction[i - 1] + (a[i - 1] + a[i]) * gate_spacing_km;
    }
    z_dbz
        .iter()
        .zip(two_way_correction)
        .map(|(&z, corr)| z + corr)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BETA: f64 = 0.64884;
    const A_COEF_C_BAND: f64 = 0.08;
    const DR_KM: f64 = 0.150;

    fn bell_profile_dbz(n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| {
                let r = i as f64 * DR_KM;
                20.0 + 25.0 * (-0.5 * ((r - 15.0) / 5.0f64).powi(2)).exp()
            })
            .collect()
    }

    /// Identidad de autoconsistencia del método (ver el doc-comment del
    /// módulo): la atenuación bidireccional total integrada de A(r) debe
    /// coincidir con `a_coef·ΔΦDP` para CUALQUIER forma de perfil de Z, no
    /// sólo en el caso donde el perfil "verdadero" coincide con el
    /// asumido -- es una identidad algebraica de la fórmula, no un
    /// resultado de sesgo bajo.
    #[test]
    fn self_consistency_two_way_pia_matches_a_coef_times_delta_phidp() {
        let z_dbz = bell_profile_dbz(200);
        for &delta_phidp in &[0.5, 5.0, 20.0, 80.0] {
            let a = zphi_specific_attenuation(&z_dbz, DR_KM, BETA, A_COEF_C_BAND, delta_phidp);
            let one_way: f64 = a.windows(2).map(|w| 0.5 * (w[0] + w[1]) * DR_KM).sum();
            let two_way_pia = 2.0 * one_way;
            let expected = A_COEF_C_BAND * delta_phidp;
            // Tolerancia dominada por el error de discretización del
            // trapecio a `DR_KM`, no por la identidad en sí (que es exacta
            // en el límite continuo -- ver el doc-comment del módulo); crece
            // con la atenuación total porque también crece la curvatura de
            // `Z(r)^beta` que el trapecio aproxima.
            let tolerance = (0.002 * expected.abs()).max(1e-3);
            assert!(
                (two_way_pia - expected).abs() < tolerance,
                "delta_phidp={delta_phidp}: PIA estimado={two_way_pia}, esperado={expected}, tolerancia={tolerance}"
            );
        }
    }

    #[test]
    fn zero_delta_phidp_leaves_profile_unchanged() {
        let z_dbz = bell_profile_dbz(50);
        let corrected = zphi_correct_dbz(&z_dbz, DR_KM, BETA, A_COEF_C_BAND, 0.0);
        for (a, b) in z_dbz.iter().zip(corrected.iter()) {
            assert!((a - b).abs() < 1e-9);
        }
    }

    #[test]
    fn negative_delta_phidp_is_censored_to_no_correction() {
        let z_dbz = bell_profile_dbz(50);
        let corrected = zphi_correct_dbz(&z_dbz, DR_KM, BETA, A_COEF_C_BAND, -5.0);
        for (a, b) in z_dbz.iter().zip(corrected.iter()) {
            assert!((a - b).abs() < 1e-9);
        }
    }

    #[test]
    fn matched_model_recovers_true_profile() {
        let n = 200;
        let z_true_dbz = bell_profile_dbz(n);
        let alpha_za = 0.0002;
        let a_true: Vec<f64> = z_true_dbz
            .iter()
            .map(|&dbz| alpha_za * 10f64.powf(dbz / 10.0).powf(BETA))
            .collect();
        let mut cum = vec![0.0; n];
        for i in 1..n {
            cum[i] = cum[i - 1] + 0.5 * (a_true[i - 1] + a_true[i]) * DR_KM;
        }
        let z_meas_dbz: Vec<f64> = z_true_dbz
            .iter()
            .zip(&cum)
            .map(|(&z, &c)| z - 2.0 * c)
            .collect();
        let delta_phidp = 2.0 * cum[n - 1] / A_COEF_C_BAND;

        let corrected = zphi_correct_dbz(&z_meas_dbz, DR_KM, BETA, A_COEF_C_BAND, delta_phidp);
        let max_bias = corrected
            .iter()
            .zip(&z_true_dbz)
            .skip(5)
            .take(n - 10)
            .map(|(&c, &t)| (c - t).abs())
            .fold(0.0, f64::max);
        assert!(max_bias < 0.05, "sesgo maximo interior: {max_bias} dB");
    }

    #[test]
    #[should_panic(expected = "al menos un intervalo")]
    fn single_gate_panics() {
        zphi_specific_attenuation(&[30.0], DR_KM, BETA, A_COEF_C_BAND, 5.0);
    }
}
