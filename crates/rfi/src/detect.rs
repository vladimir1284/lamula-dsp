//! Detección de líneas de RFI sobre un periodograma ya calculado
//! (`tools/oracles/rfi_filtrado.ipynb`): exceso sobre la mediana y anchura
//! angosta contigua al pico.

/// Exceso mínimo sobre la mediana del espectro, en dB, para considerar un
/// bin candidato a RFI. Calibrado en el oráculo sobre 300 escenarios
/// limpios con `M=256`: el peor pico angosto producido por azar por el
/// ruido llegó a 27.6 dB sobre la mediana (problema de comparaciones
/// múltiples, no defecto del detector); 28 dB deja margen sin perder
/// sensibilidad frente a RFI inyectada 15-20 dB sobre el pico del meteoro.
pub const DEFAULT_RFI_MEDIAN_DB: f64 = 28.0;

/// Anchura máxima, en bins, de la región contigua elevada alrededor de un
/// candidato para clasificarlo como RFI en vez de eco meteorológico: el
/// lóbulo principal de la ventana de Hann, no la anchura Doppler de un eco
/// real (que crece con `M`).
pub const DEFAULT_RFI_WIDTH_MAX_BINS: usize = 3;

/// Anchura de la región contigua alrededor de `peak` que se mantiene por
/// encima de `P[peak] / drop_factor`, buscando hacia ambos lados sin cruzar
/// un cuarto de vuelta completa del espectro. Devuelve `(anchura, lo, hi)`
/// con `lo`/`hi` en índice de bin nativo (ya envueltos módulo `M`).
pub fn spike_width(p: &[f64], peak: usize, drop_factor: f64) -> (usize, usize, usize) {
    let m = p.len() as i64;
    let quarter = m / 4;
    let thresh = p[peak] / drop_factor;
    let peak_i = peak as i64;

    let mut lo = peak_i;
    while p[(lo - 1).rem_euclid(m) as usize] > thresh && (peak_i - (lo - 1)) < quarter {
        lo -= 1;
    }
    let mut hi = peak_i;
    while p[(hi + 1).rem_euclid(m) as usize] > thresh && ((hi + 1) - peak_i) < quarter {
        hi += 1;
    }

    let width = (hi - lo + 1) as usize;
    (width, lo.rem_euclid(m) as usize, hi.rem_euclid(m) as usize)
}

fn median(p: &[f64]) -> f64 {
    let mut sorted = p.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("periodograma con NaN"));
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Máscara de bins atribuidos a RFI: candidatos que superan la mediana del
/// espectro en `median_db` dB y cuya región elevada contigua (criterio de
/// [`spike_width`] con `drop_factor=2.0`) mide como mucho `width_max_bins`.
/// Recorre los candidatos en orden creciente de bin y salta los ya
/// marcados por un candidato anterior, igual que el oráculo.
pub fn detect_rfi_mask(p: &[f64], median_db: f64, width_max_bins: usize) -> Vec<bool> {
    let m = p.len();
    let median_p = median(p);
    let thresh = median_p * 10f64.powf(median_db / 10.0);

    let mut mask = vec![false; m];
    for c in 0..m {
        if p[c] <= thresh || mask[c] {
            continue;
        }
        let (width, lo, hi) = spike_width(p, c, 2.0);
        if width <= width_max_bins {
            let m_i = m as i64;
            let lo_i = lo as i64;
            let hi_i = if (hi as i64) < lo_i {
                hi as i64 + m_i
            } else {
                hi as i64
            };
            for i in lo_i..=hi_i {
                mask[i.rem_euclid(m_i) as usize] = true;
            }
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_narrow_spike_above_median() {
        let mut p = vec![0.01; 64];
        p[10] = 100.0;
        let mask = detect_rfi_mask(&p, DEFAULT_RFI_MEDIAN_DB, DEFAULT_RFI_WIDTH_MAX_BINS);
        assert!(mask[10]);
        assert_eq!(mask.iter().filter(|&&m| m).count(), 1);
    }

    #[test]
    fn ignores_flat_spectrum() {
        let p = vec![0.01; 64];
        let mask = detect_rfi_mask(&p, DEFAULT_RFI_MEDIAN_DB, DEFAULT_RFI_WIDTH_MAX_BINS);
        assert!(mask.iter().all(|&m| !m));
    }

    #[test]
    fn wide_bump_is_not_flagged_as_rfi() {
        // Simula un eco meteorológico ancho: varios bins consecutivos
        // elevados por encima de la mediana, anchura mayor que el máximo
        // permitido para RFI.
        let mut p = vec![0.01; 64];
        for x in p.iter_mut().take(16).skip(8) {
            *x = 50.0;
        }
        let mask = detect_rfi_mask(&p, DEFAULT_RFI_MEDIAN_DB, 3);
        assert!(mask.iter().all(|&m| !m));
    }
}
