//! Filtro de clutter en el dominio espectral: notch clásico y GMAP
//! (Siggia & Passarelli 2004), más extracción de momentos sobre el espectro
//! corregido.

/// Recorta a cero las líneas espectrales dentro de la banda de clutter.
pub fn notch_filter(p: &[f64], clutter_mask: &[bool]) -> Vec<f64> {
    assert_eq!(
        p.len(),
        clutter_mask.len(),
        "P y la máscara deben medir igual"
    );
    p.iter()
        .zip(clutter_mask)
        .map(|(&pwr, &clutter)| if clutter { 0.0 } else { pwr })
        .collect()
}

pub struct GmapFilterResult {
    /// Periodograma corregido: fuera de la banda de clutter, igual al de
    /// entrada; dentro, interpolado por el modelo gaussiano (o recortado a
    /// cero si el ajuste no fue fiable).
    pub filtered: Vec<f64>,
    /// `true` si el ajuste gaussiano se aplicó; `false` si se degradó a
    /// notch por falta de bins de señal o por curvatura no cóncava.
    pub fit_ok: bool,
}

/// Resuelve el sistema normal `3x3` de mínimos cuadrados `A^T·A·c = A^T·b`
/// por eliminación gaussiana con pivoteo parcial. Devuelve `None` si el
/// sistema es singular (columnas de diseño degeneradas).
fn solve_3x3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let pivot_row =
            (col..3).max_by(|&r1, &r2| a[r1][col].abs().partial_cmp(&a[r2][col].abs()).unwrap())?;
        if a[pivot_row][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, pivot_row);
        b.swap(col, pivot_row);

        for row in (col + 1)..3 {
            let factor = a[row][col] / a[col][col];
            let pivot_row = a[col];
            for (k, pivot_val) in pivot_row.iter().enumerate().skip(col) {
                a[row][k] -= factor * pivot_val;
            }
            b[row] -= factor * b[col];
        }
    }

    let mut c = [0.0; 3];
    for row in (0..3).rev() {
        let sum: f64 = (row + 1..3).map(|k| a[row][k] * c[k]).sum();
        c[row] = (b[row] - sum) / a[row][row];
    }
    Some(c)
}

/// Ajusta `log(P) = c0 + c1·v + c2·v^2` por mínimos cuadrados sobre los
/// pares `(v[i], logp[i])`.
fn fit_quadratic_log(v: &[f64], logp: &[f64]) -> Option<[f64; 3]> {
    let mut ata = [[0.0f64; 3]; 3];
    let mut atb = [0.0f64; 3];
    for (&vi, &yi) in v.iter().zip(logp) {
        let row = [1.0, vi, vi * vi];
        for i in 0..3 {
            atb[i] += row[i] * yi;
            for j in 0..3 {
                ata[i][j] += row[i] * row[j];
            }
        }
    }
    solve_3x3(ata, atb)
}

/// GMAP: interpola la banda de clutter con un modelo gaussiano ajustado a
/// los bins fuera de la banda que superan `margin·noise_thresh_per_bin`. Se
/// degrada a notch (banda a cero) si hay menos de 4 bins de señal fiables o
/// si la curvatura ajustada no es cóncava (`c2 >= 0`, ajuste no fiable) —
/// declarado explícitamente en vez de propagar NaN. Una máscara de clutter
/// vacía (mapa de clutter: celda sin clutter esperado) recorre el mismo
/// camino que "sin filtro", porque no hay ningún bin que reemplazar.
pub fn gmap_filter(
    p: &[f64],
    v_k: &[f64],
    clutter_mask: &[bool],
    noise_thresh_per_bin: f64,
    margin: f64,
) -> GmapFilterResult {
    assert_eq!(
        p.len(),
        v_k.len(),
        "P y el eje de velocidad deben medir igual"
    );
    assert_eq!(
        p.len(),
        clutter_mask.len(),
        "P y la máscara deben medir igual"
    );

    let mut filtered = p.to_vec();
    let signal_idx: Vec<usize> = (0..p.len())
        .filter(|&i| !clutter_mask[i] && p[i] > margin * noise_thresh_per_bin)
        .collect();

    if signal_idx.len() < 4 {
        for (i, &clutter) in clutter_mask.iter().enumerate() {
            if clutter {
                filtered[i] = 0.0;
            }
        }
        return GmapFilterResult {
            filtered,
            fit_ok: false,
        };
    }

    let v_signal: Vec<f64> = signal_idx.iter().map(|&i| v_k[i]).collect();
    let logp_signal: Vec<f64> = signal_idx.iter().map(|&i| p[i].max(1e-300).ln()).collect();

    let coeffs = fit_quadratic_log(&v_signal, &logp_signal);
    let concave_fit = coeffs.filter(|c| c[2] < 0.0);

    match concave_fit {
        Some(c) => {
            for (i, &clutter) in clutter_mask.iter().enumerate() {
                if clutter {
                    let v = v_k[i];
                    filtered[i] = (c[0] + c[1] * v + c[2] * v * v).exp();
                }
            }
            GmapFilterResult {
                filtered,
                fit_ok: true,
            }
        }
        None => {
            for (i, &clutter) in clutter_mask.iter().enumerate() {
                if clutter {
                    filtered[i] = 0.0;
                }
            }
            GmapFilterResult {
                filtered,
                fit_ok: false,
            }
        }
    }
}

pub struct SpectralMoments {
    pub power_linear: f64,
    pub velocity_mps: Option<f64>,
    pub spectrum_width_mps: Option<f64>,
}

/// Potencia, velocidad y ancho por momentos directos sobre un espectro ya
/// corregido (por GMAP o por notch), tras restar el umbral de ruido por bin
/// y recentrar circularmente alrededor del pico -- misma técnica que
/// `docs/algorithms/estimador-espectral.md` para el eco partido por la
/// Nyquist.
pub fn moments_from_spectrum(
    p_filtered: &[f64],
    v_k: &[f64],
    bin_spacing: f64,
    noise_thresh_per_bin: f64,
) -> SpectralMoments {
    assert_eq!(
        p_filtered.len(),
        v_k.len(),
        "P y el eje de velocidad deben medir igual"
    );
    let m = p_filtered.len();

    let p_sub: Vec<f64> = p_filtered
        .iter()
        .map(|&p| (p - noise_thresh_per_bin).max(0.0))
        .collect();
    let total: f64 = p_sub.iter().sum();
    if total <= 0.0 {
        return SpectralMoments {
            power_linear: 0.0,
            velocity_mps: None,
            spectrum_width_mps: None,
        };
    }

    let peak = (0..m)
        .max_by(|&a, &b| p_sub[a].partial_cmp(&p_sub[b]).unwrap())
        .expect("periodograma no vacío");
    let half = m as i64 / 2;

    let v_axis: Vec<f64> = (0..m)
        .map(|i| {
            let raw_offset = i as i64 - peak as i64;
            let offset = (raw_offset + half).rem_euclid(m as i64) - half;
            v_k[peak] + offset as f64 * bin_spacing
        })
        .collect();

    let v_mean = p_sub.iter().zip(&v_axis).map(|(&p, &v)| p * v).sum::<f64>() / total;
    let variance = p_sub
        .iter()
        .zip(&v_axis)
        .map(|(&p, &v)| p * (v - v_mean).powi(2))
        .sum::<f64>()
        / total;

    SpectralMoments {
        power_linear: total,
        velocity_mps: Some(v_mean),
        spectrum_width_mps: Some(variance.sqrt()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notch_zeroes_only_clutter_bins() {
        let p = vec![1.0, 2.0, 3.0, 4.0];
        let mask = vec![false, true, true, false];
        let filtered = notch_filter(&p, &mask);
        assert_eq!(filtered, vec![1.0, 0.0, 0.0, 4.0]);
    }

    #[test]
    fn empty_clutter_mask_leaves_spectrum_untouched() {
        let p = vec![0.1, 5.0, 0.2, 0.3, 0.1, 4.0, 0.2, 0.1];
        let v_k = vec![-4.0, -3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0];
        let mask = vec![false; 8];
        let result = gmap_filter(&p, &v_k, &mask, 0.01, 3.0);
        assert_eq!(result.filtered, p);
    }

    #[test]
    fn too_few_signal_bins_degrades_to_notch() {
        let p = vec![0.02, 0.02, 0.02, 0.02];
        let v_k = vec![-2.0, -1.0, 0.0, 1.0];
        let mask = vec![false, true, true, false];
        let result = gmap_filter(&p, &v_k, &mask, 0.01, 3.0);
        assert!(!result.fit_ok);
        assert_eq!(result.filtered, vec![0.02, 0.0, 0.0, 0.02]);
    }
}
