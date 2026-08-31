//! Periodograma de una ráfaga y potencia total en el dominio del tiempo.

use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

/// Potencia total medida `R(0) = E[|y[n]|^2]`, estimada como la media
/// muestral sobre la ráfaga.
pub fn total_power(y: &[Complex64]) -> f64 {
    assert!(!y.is_empty(), "la ráfaga no puede estar vacía");
    y.iter().map(|s| s.norm_sqr()).sum::<f64>() / y.len() as f64
}

/// Periodograma `P[k] = |FFT(y)[k]|^2 / M^2` de una ráfaga, en el mismo orden
/// de bin nativo de la FFT que produce `rustfft` (igual que `numpy.fft.fft`:
/// transformada directa sin normalizar). Reparte la misma potencia total que
/// la serie temporal — `mean(P) == total_power(y)` — porque Parseval no
/// depende de la fase de ningún bin.
pub fn periodogram(y: &[Complex64]) -> Vec<f64> {
    let m = y.len();
    assert!(m > 0, "la ráfaga no puede estar vacía");

    let mut buf: Vec<Complex64> = y.to_vec();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(m);
    fft.process(&mut buf);

    let m2 = (m * m) as f64;
    buf.iter().map(|c| c.norm_sqr() / m2).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn periodogram_conserves_total_power() {
        let mut rng = StdRng::seed_from_u64(7);
        let params = lamula_simulator::CellParams {
            power_s: 1.0,
            mean_v: 5.0,
            sigma_v: 1.5,
            wavelength_m: 0.10,
            prt_s: 1.0e-3,
            m: 128,
            noise_floor: 0.3,
        };
        let y = lamula_simulator::generate_cell(&params, &mut rng);

        let lhs = total_power(&y);
        let rhs: f64 = periodogram(&y).iter().sum();

        assert!(
            (lhs - rhs).abs() < 1e-9,
            "el periodograma no reparte la misma potencia total que la serie temporal: media(|y|^2)={lhs} suma(periodograma)={rhs}"
        );
    }
}
