//! Estimación objetiva del suelo de ruido, Hildebrand & Sekhon (1974).
//!
//! Ecuación 9 del paper: se ordenan las líneas del periodograma de menor a
//! mayor potencia y se acumulan; mientras el conjunto acumulado se comporte
//! como ruido blanco (la razón entre `npts·Σp²` y `(Σp)²` se mantiene por
//! debajo de `1 + 1/navg`), se sigue incluyendo la siguiente línea. En cuanto
//! deja de cumplirse, esa línea (y todo lo que queda por encima) es señal, no
//! ruido, y se descarta del acumulado.

use rustfft::num_complex::Complex64;

use crate::periodogram::periodogram;

/// Media de ruido por bin sobre un periodograma ya calculado (`p`, `M`
/// valores). `navg` es el número de periodogramas independientes promediados
/// para producir `p`; con una única ráfaga es `1.0`.
pub fn hildebrand_sekhon(p: &[f64], navg: f64) -> f64 {
    assert!(!p.is_empty(), "el periodograma no puede estar vacío");
    assert!(navg > 0.0, "navg debe ser positivo");

    let mut sorted = p.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("periodograma con NaN"));

    let rtest = 1.0 + 1.0 / navg;
    let mut nnoise = sorted.len();
    let mut sum1 = 0.0f64;
    let mut sum2 = 0.0f64;

    for (i, &pwr) in sorted.iter().enumerate() {
        let npts = (i + 1) as f64;
        sum1 += pwr;
        sum2 += pwr * pwr;
        if npts * sum2 < sum1 * sum1 * rtest {
            nnoise = i + 1;
        } else {
            sum1 -= pwr;
            break;
        }
    }

    sum1 / nnoise as f64
}

/// Estima `N`, la potencia total de ruido (no por bin) de una ráfaga, vía
/// HS74 sobre su periodograma.
pub fn noise_floor_estimate(y: &[Complex64]) -> f64 {
    let m = y.len();
    hildebrand_sekhon(&periodogram(y), 1.0) * m as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamula_simulator::CellParams;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// Contraste directo con `tools/oracles/ruido_y_umbrales.ipynb` (celda de
    /// `NOISE_FLOOR = 0.3`, `M = 256`, `N_TRIALS_PURE = 4000`,
    /// `BIAS_TOLERANCE_DB = 1.0`, sesgo conocido de HS74 con `navg=1` ≈ 0.4 dB).
    #[test]
    fn recovers_injected_noise_floor_on_pure_noise() {
        const NOISE_FLOOR: f64 = 0.3;
        const M: usize = 256;
        const N_TRIALS: usize = 4000;
        const BIAS_TOLERANCE_DB: f64 = 1.0;

        let mut rng = StdRng::seed_from_u64(20260901);
        let params = CellParams {
            power_s: 0.0,
            mean_v: 0.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.0e-3,
            m: M,
            noise_floor: NOISE_FLOOR,
        };

        let mean_est: f64 = (0..N_TRIALS)
            .map(|_| {
                let y = lamula_simulator::generate_cell(&params, &mut rng);
                noise_floor_estimate(&y)
            })
            .sum::<f64>()
            / N_TRIALS as f64;

        let bias_db = 10.0 * (mean_est / NOISE_FLOOR).log10();
        assert!(
            bias_db.abs() < BIAS_TOLERANCE_DB,
            "HS74 sobre ruido puro no recupera N inyectado: N={NOISE_FLOOR} estimado={mean_est:.4} sesgo={bias_db:.3} dB"
        );
    }
}
