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
//! **Verificada 2026-09-18 contra el paper original (Testud et al. 2000).**
//! `zphi_specific_attenuation` implementa exactamente las ecuaciones (19),
//! (20), (23) y (24) del paper ("formulación simple", exponente de la
//! relación KDP-A forzado a 1 — Ec. 21); `zphi_correct_dbz` es equivalente
//! por derivación directa a su ecuación (25) (integración numérica de `A(r)`
//! en vez de la forma cerrada — ver `docs/algorithms/atenuacion-zphi.md`
//! §"Cómo funciona" para el desarrollo completo). La identidad de
//! autoconsistencia (`self_consistency_two_way_pia_matches_a_coef_times_delta_phidp`)
//! también se deriva directamente de la Ec. (21) del paper, no es sólo una
//! propiedad interna verificada contra sí misma.
//!
//! **`zphi_specific_attenuation_accurate`/`zphi_correct_dbz_accurate` —
//! "formulación más precisa" del mismo paper (§c, "A more accurate
//! formulation", Ecs. 26-27), agregada 2026-09-18 porque la instalación de
//! referencia de este repositorio es banda S y el propio paper advierte que
//! ahí NO conviene forzar a 1 el exponente `b` de la relación KDP-A (Tabla
//! 1c: `b≈0.97-0.99` en X/C, pero `b≈1.18` en S) — la formulación simple de
//! arriba se degrada en banda S por esa razón, no por un error de esta
//! implementación (ver "Caveat" en `docs/algorithms/atenuacion-zphi.md`
//! §"Cómo funciona"). A diferencia de la simple, no puede eliminar `N*₀`
//! (parámetro de intercepto normalizado de la DSD, Tabla 1) algebraicamente:
//! resuelve `A(r0)` y `N*₀` como sistema acoplado (Ecs. 26+27) por
//! bisección numérica sobre `A(r0)` — el paper mismo dice "puede resolverse
//! con una técnica numérica estándar", sin dar una; la bisección asume que
//! el residuo de la Ec. (27) es monótono creciente en `A(r0)`, verificado
//! empíricamente en el oráculo (`tools/oracles/atenuacion_zphi.ipynb`) para
//! los coeficientes de banda S, no demostrado analíticamente para todo
//! rango de coeficientes. Necesita CUATRO coeficientes de la Tabla 1 del
//! paper en vez de dos: `beta_az`/`a_az` (Tabla 1a, relación A-Z) y
//! `beta_kdp`/`a_kdp` (Tabla 1c, relación KDP-A), por banda y canal (H por
//! convención, igual que la formulación simple) — no vienen de Gu et al.
//! (2011)/Py-ART como los de la formulación simple, que no cubre esta
//! variante.
//!
//! Precondición de todas las funciones (responsabilidad de quien llama,
//! mismo criterio que `lamula_calibration::power_to_dbz`): `z_dbz` es un
//! tramo YA censurado — sin `NaN`, con eco detectable — `delta_phidp_deg` ya
//! no-negativo no es precondición: un valor negativo (ruido de fase en
//! tramos sin atenuación real) se trata como "sin atenuación medible" en vez
//! de propagar una corrección de signo equivocado, coherente con el
//! criterio de censura del resto del pipeline ("censura, no corrige" ante lo
//! que no se puede medir con confianza).

/// `(Za(s)^beta, prefijo I(r1,s) = 0.46*beta*∫_{r1}^s Za^beta dr, regla del
/// trapecio)` — Ec. (20) del paper, compartido por la formulación simple y
/// la acorde (ambas necesitan exactamente esta misma integral, sólo con
/// `beta` distinto: `beta` de la relación Z-atenuación en los dos casos, no
/// el de la relación KDP-A).
fn zphi_prefix_integral(z_dbz: &[f64], gate_spacing_km: f64, beta: f64) -> (Vec<f64>, Vec<f64>) {
    let n = z_dbz.len();
    let z_beta: Vec<f64> = z_dbz
        .iter()
        .map(|&dbz| {
            let z_linear = 10f64.powf(dbz / 10.0);
            assert!(z_linear > 0.0, "z_dbz debe ser finito (tramo sin censurar)");
            z_linear.powf(beta)
        })
        .collect();
    let mut prefix = vec![0.0; n];
    for i in 1..n {
        prefix[i] =
            prefix[i - 1] + 0.46 * beta * 0.5 * (z_beta[i - 1] + z_beta[i]) * gate_spacing_km;
    }
    (z_beta, prefix)
}

/// Camino bidireccional acumulado [dB] de un perfil `a` [dB/km] ya conocido:
/// `2·∫_{r1}^{r_i} a(r) dr` por trapecio en cada celda `i`, factor 2 por el
/// viaje de ida y vuelta. Compartido por las dos formulaciones: una vez que
/// se tiene el perfil `A(r)` (por cualquiera de las dos vías), la corrección
/// de reflectividad es la misma relación física (`Ze[dB] = Za[dB] +
/// 2∫A dr`), no depende de cómo se resolvió `A(r)` — ver el doc-comment del
/// módulo para la equivalencia con la Ec. (25) del paper.
fn two_way_pia_correction(a: &[f64], gate_spacing_km: f64) -> Vec<f64> {
    let n = a.len();
    let mut corr = vec![0.0; n];
    for i in 1..n {
        corr[i] = corr[i - 1] + (a[i - 1] + a[i]) * gate_spacing_km;
    }
    corr
}

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
    let (z_beta, prefix) = zphi_prefix_integral(z_dbz, gate_spacing_km, beta);
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
    let corr = two_way_pia_correction(&a, gate_spacing_km);
    z_dbz.iter().zip(corr).map(|(&z, c)| z + c).collect()
}

/// Perfil de atenuación específica A(r) [dB/km], "formulación más precisa"
/// de Testud et al. (2000) §c (Ecs. 26-27) — ver el doc-comment del módulo
/// para cuándo usar ésta en vez de [`zphi_specific_attenuation`] (banda S) y
/// para el origen de los cuatro coeficientes. `beta_az`/`a_az` son la
/// relación Z-atenuación (Tabla 1a: `A = a_az·(N*₀)^(1-beta_az)·Za^beta_az`,
/// mismo `beta_az` que la formulación simple llama sólo `beta`);
/// `beta_kdp`/`a_kdp` son la relación KDP-atenuación (Tabla 1c: `KDP =
/// a_kdp·(N*₀)^(1-beta_kdp)·A^beta_kdp`, la que la formulación simple fuerza
/// a `beta_kdp=1` para poder eliminar `N*₀` algebraicamente).
pub fn zphi_specific_attenuation_accurate(
    z_dbz: &[f64],
    gate_spacing_km: f64,
    beta_az: f64,
    a_az: f64,
    beta_kdp: f64,
    a_kdp: f64,
    delta_phidp_deg: f64,
) -> Vec<f64> {
    assert!(
        z_dbz.len() >= 2,
        "hace falta al menos un intervalo (2 celdas) para integrar"
    );
    assert!(gate_spacing_km > 0.0, "gate_spacing_km debe ser positivo");
    assert!(
        beta_az > 0.0 && beta_az < 1.0,
        "beta_az (relación A-Z, Tabla 1a) debe estar en (0,1) para que N*₀ \
         quede despejable en Ec. (26) -- todas las filas de la Tabla 1a del \
         paper cumplen esto"
    );
    assert!(a_az > 0.0, "a_az debe ser positivo");
    assert!(
        beta_kdp > 0.0,
        "beta_kdp (relación KDP-A, Tabla 1c) debe ser positivo"
    );
    assert!(a_kdp > 0.0, "a_kdp debe ser positivo");

    let n = z_dbz.len();
    let (z_beta, prefix) = zphi_prefix_integral(z_dbz, gate_spacing_km, beta_az);
    let i_total = prefix[n - 1];
    assert!(
        i_total > 0.0,
        "el tramo no tiene señal detectable (Z=0 en todas las celdas); sin eso el método no está definido"
    );
    let za0_beta = z_beta[n - 1];

    let delta_phidp_deg = delta_phidp_deg.max(0.0);
    let target_half_dphi = 0.5 * delta_phidp_deg;
    if target_half_dphi == 0.0 {
        // ΔΦDP censurado a 0 -- ver el doc-comment del módulo. A(r0)=0
        // resuelve trivialmente Ec. (27) (los dos lados son cero), así que
        // se puede devolver el perfil nulo sin arrancar la bisección.
        return vec![0.0; n];
    }

    // Ec. (27) sustituyendo N*₀ de la Ec. (26): residuo en función del único
    // desconocido A(r0). Monótono creciente en A(r0) (más atenuación
    // acumulada en la celda de referencia implica más ΔΦDP explicado, ver
    // el doc-comment del módulo) -- verificado empíricamente en el oráculo
    // para banda S, no demostrado aquí para todo `beta_az`/`beta_kdp`.
    let residual = |a0: f64| -> f64 {
        let d0 = za0_beta + a0 * i_total;
        let n0_star = (a0 / (a_az * d0)).powf(1.0 / (1.0 - beta_az));
        let j_integral: f64 = (0..n)
            .map(|i| {
                let tail = i_total - prefix[i];
                let denom = za0_beta + a0 * tail;
                (z_beta[i] / denom).powf(beta_kdp)
            })
            .collect::<Vec<f64>>()
            .windows(2)
            .map(|w| 0.5 * (w[0] + w[1]) * gate_spacing_km)
            .sum();
        let lhs = a_kdp * n0_star.powf(1.0 - beta_kdp) * a0.powf(beta_kdp) * j_integral;
        lhs - target_half_dphi
    };

    // Bracket ampliado por duplicación (el residuo en `lo` casi cero es
    // negativo -- ver el doc-comment) más bisección: sin derivada, y esta
    // ecuación no tiene forma cerrada, coherente con lo que dice el propio
    // paper ("may be solved using a standard numerical technique").
    let mut lo = 1e-9_f64;
    let mut hi = 1.0_f64;
    let mut f_hi = residual(hi);
    let mut expansions = 0;
    while f_hi < 0.0 && expansions < 40 {
        hi *= 4.0;
        f_hi = residual(hi);
        expansions += 1;
    }
    assert!(
        f_hi >= 0.0,
        "no se encontró A(r0) que sature la restricción de ΔΦDP tras {expansions} \
         duplicaciones del bracket -- revisar delta_phidp_deg/coeficientes"
    );
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if residual(mid) > 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let a0 = 0.5 * (lo + hi);

    (0..n)
        .map(|i| {
            let tail = i_total - prefix[i];
            let denom = za0_beta + a0 * tail;
            a0 * z_beta[i] / denom
        })
        .collect()
}

/// Reflectividad corregida de atenuación [dBZ], formulación más precisa —
/// mismo camino bidireccional que [`zphi_correct_dbz`], sobre el perfil de
/// [`zphi_specific_attenuation_accurate`]. Ver el doc-comment del módulo
/// para las precondiciones.
pub fn zphi_correct_dbz_accurate(
    z_dbz: &[f64],
    gate_spacing_km: f64,
    beta_az: f64,
    a_az: f64,
    beta_kdp: f64,
    a_kdp: f64,
    delta_phidp_deg: f64,
) -> Vec<f64> {
    let a = zphi_specific_attenuation_accurate(
        z_dbz,
        gate_spacing_km,
        beta_az,
        a_az,
        beta_kdp,
        a_kdp,
        delta_phidp_deg,
    );
    let corr = two_way_pia_correction(&a, gate_spacing_km);
    z_dbz.iter().zip(corr).map(|(&z, c)| z + c).collect()
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

    // Testud et al. (2000) Tabla 1, canal H, banda S -- ver el doc-comment
    // del módulo ("formulación más precisa") y `docs/algorithms/
    // atenuacion-zphi.md` §"Cómo funciona".
    const BETA_AZ_S: f64 = 0.701; // Tabla 1a
    const A_AZ_S: f64 = 9.28e-8; // Tabla 1a
    const BETA_KDP_S: f64 = 1.18; // Tabla 1c
    const A_KDP_S: f64 = 1.31e3; // Tabla 1c

    /// Caso de modelo acoplado a coeficientes de banda S: la atenuación Y el
    /// KDP "verdaderos" se generan con la MISMA relación con `N*₀` que
    /// asume la formulación acorde (Ecs. 26-27), a diferencia de
    /// [`matched_model_recovers_true_profile`] (formulación simple) que sólo
    /// necesita la relación A-Z. `N*₀=0.8e7` (Marshall-Palmer, mismo valor
    /// que usa el paper como referencia). Contraste directo con el oráculo
    /// (`tools/oracles/atenuacion_zphi.ipynb`, celda de banda S): mismos
    /// coeficientes, mismo perfil, mismo sesgo esperado (< 0.01 dB).
    #[test]
    fn matched_model_recovers_true_profile_accurate_s_band() {
        const N: usize = 60;
        const DR_KM2: f64 = 0.5;
        const N0_STAR_TRUE: f64 = 0.8e7;

        let z_true_dbz: Vec<f64> = (0..N)
            .map(|i| {
                let r = i as f64 * DR_KM2;
                20.0 + 25.0 * (-0.5 * ((r - 15.0) / 6.0f64).powi(2)).exp()
            })
            .collect();
        let a_true: Vec<f64> = z_true_dbz
            .iter()
            .map(|&dbz| {
                A_AZ_S
                    * N0_STAR_TRUE.powf(1.0 - BETA_AZ_S)
                    * (10f64.powf(dbz / 10.0)).powf(BETA_AZ_S)
            })
            .collect();
        let mut cum_a = vec![0.0; N];
        for i in 1..N {
            cum_a[i] = cum_a[i - 1] + 0.5 * (a_true[i - 1] + a_true[i]) * DR_KM2;
        }
        let z_meas_dbz: Vec<f64> = z_true_dbz
            .iter()
            .zip(&cum_a)
            .map(|(&z, &c)| z - 2.0 * c)
            .collect();

        let kdp_true: Vec<f64> = a_true
            .iter()
            .map(|&a| A_KDP_S * N0_STAR_TRUE.powf(1.0 - BETA_KDP_S) * a.powf(BETA_KDP_S))
            .collect();
        let mut cum_kdp = vec![0.0; N];
        for i in 1..N {
            cum_kdp[i] = cum_kdp[i - 1] + 0.5 * (kdp_true[i - 1] + kdp_true[i]) * DR_KM2;
        }
        let delta_phidp = 2.0 * cum_kdp[N - 1];

        let corrected = zphi_correct_dbz_accurate(
            &z_meas_dbz,
            DR_KM2,
            BETA_AZ_S,
            A_AZ_S,
            BETA_KDP_S,
            A_KDP_S,
            delta_phidp,
        );
        let max_bias = corrected
            .iter()
            .zip(&z_true_dbz)
            .skip(5)
            .take(N - 10)
            .map(|(&c, &t)| (c - t).abs())
            .fold(0.0, f64::max);
        let uncorrected_bias = z_meas_dbz
            .iter()
            .zip(&z_true_dbz)
            .skip(5)
            .take(N - 10)
            .map(|(&c, &t)| (c - t).abs())
            .fold(0.0, f64::max);
        assert!(
            max_bias < 0.01,
            "sesgo maximo interior (banda S, formulacion acorde): {max_bias} dB"
        );
        assert!(
            max_bias < uncorrected_bias,
            "la correccion debe reducir el sesgo frente a la medida sin corregir"
        );
    }

    /// Sobre el mismo escenario que la prueba anterior: la formulación
    /// SIMPLE (`beta=0.64884`/`a_coef` de Gu et al. 2011 para banda S, los
    /// valores por defecto de este crate antes de la formulación acorde) se
    /// degrada más — no es un error, es justo el caveat que Testud et al.
    /// (2000) §c señala para banda S (forzar a 1 el exponente de la
    /// relación KDP-A no es buena aproximación ahí, Tabla 1c: `b≈1.18`).
    #[test]
    fn simple_formulation_is_more_biased_than_accurate_on_s_band_truth() {
        const N: usize = 60;
        const DR_KM2: f64 = 0.5;
        const N0_STAR_TRUE: f64 = 0.8e7;
        const A_COEF_S_SIMPLE: f64 = 0.02; // Gu et al. (2011), ver la página del algoritmo

        let z_true_dbz: Vec<f64> = (0..N)
            .map(|i| {
                let r = i as f64 * DR_KM2;
                20.0 + 25.0 * (-0.5 * ((r - 15.0) / 6.0f64).powi(2)).exp()
            })
            .collect();
        let a_true: Vec<f64> = z_true_dbz
            .iter()
            .map(|&dbz| {
                A_AZ_S
                    * N0_STAR_TRUE.powf(1.0 - BETA_AZ_S)
                    * (10f64.powf(dbz / 10.0)).powf(BETA_AZ_S)
            })
            .collect();
        let mut cum_a = vec![0.0; N];
        for i in 1..N {
            cum_a[i] = cum_a[i - 1] + 0.5 * (a_true[i - 1] + a_true[i]) * DR_KM2;
        }
        let z_meas_dbz: Vec<f64> = z_true_dbz
            .iter()
            .zip(&cum_a)
            .map(|(&z, &c)| z - 2.0 * c)
            .collect();
        let kdp_true: Vec<f64> = a_true
            .iter()
            .map(|&a| A_KDP_S * N0_STAR_TRUE.powf(1.0 - BETA_KDP_S) * a.powf(BETA_KDP_S))
            .collect();
        let mut cum_kdp = vec![0.0; N];
        for i in 1..N {
            cum_kdp[i] = cum_kdp[i - 1] + 0.5 * (kdp_true[i - 1] + kdp_true[i]) * DR_KM2;
        }
        let delta_phidp = 2.0 * cum_kdp[N - 1];

        let corrected_accurate = zphi_correct_dbz_accurate(
            &z_meas_dbz,
            DR_KM2,
            BETA_AZ_S,
            A_AZ_S,
            BETA_KDP_S,
            A_KDP_S,
            delta_phidp,
        );
        let corrected_simple =
            zphi_correct_dbz(&z_meas_dbz, DR_KM2, BETA, A_COEF_S_SIMPLE, delta_phidp);

        let bias_of = |corrected: &[f64]| -> f64 {
            corrected
                .iter()
                .zip(&z_true_dbz)
                .skip(5)
                .take(N - 10)
                .map(|(&c, &t)| (c - t).abs())
                .fold(0.0, f64::max)
        };
        let bias_accurate = bias_of(&corrected_accurate);
        let bias_simple = bias_of(&corrected_simple);
        assert!(
            bias_simple > 5.0 * bias_accurate,
            "esperado que la formulacion simple (bias={bias_simple} dB) se degrade \
             claramente frente a la acorde (bias={bias_accurate} dB) en banda S"
        );
    }

    #[test]
    fn zero_delta_phidp_leaves_profile_unchanged_accurate() {
        let z_dbz = bell_profile_dbz(50);
        let corrected =
            zphi_correct_dbz_accurate(&z_dbz, DR_KM, BETA_AZ_S, A_AZ_S, BETA_KDP_S, A_KDP_S, 0.0);
        for (a, b) in z_dbz.iter().zip(corrected.iter()) {
            assert!((a - b).abs() < 1e-9);
        }
    }

    #[test]
    #[should_panic(expected = "al menos un intervalo")]
    fn single_gate_panics_accurate() {
        zphi_specific_attenuation_accurate(
            &[30.0],
            DR_KM,
            BETA_AZ_S,
            A_AZ_S,
            BETA_KDP_S,
            A_KDP_S,
            5.0,
        );
    }
}
