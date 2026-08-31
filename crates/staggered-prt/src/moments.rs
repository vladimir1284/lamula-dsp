//! Velocidad pulse-pair sobre las dos subsecuencias de una ráfaga
//! escalonada `T1, T2, T1, T2, …` (`tools/oracles/staggered_prt.ipynb`).

use std::f64::consts::PI;

use rustfft::num_complex::Complex64;

/// Velocidades pulse-pair `(v1, v2)` de una ráfaga muestreada con periodo
/// escalonado `T1, T2, T1, T2, …` (`x[0]` y `x[1]` separados por `t1`,
/// `x[1]` y `x[2]` por `t2`, y así sucesivamente). `v1` viene de los pares
/// que empiezan en índice par (separados por `t1`); `v2`, de los que
/// empiezan en índice impar (separados por `t2`) — ambas de la misma
/// realización, sin brecha temporal entre ellas.
pub fn staggered_pulse_pair_velocities(
    x: &[Complex64],
    wavelength_m: f64,
    t1: f64,
    t2: f64,
) -> (f64, f64) {
    assert!(x.len() >= 3, "hacen falta al menos tres pulsos escalonados");
    assert!(wavelength_m > 0.0, "wavelength_m debe ser positivo");
    assert!(t1 > 0.0 && t2 > 0.0, "t1 y t2 deben ser positivos");

    let r1 = mean_lag_product(x, 0);
    let r2 = mean_lag_product(x, 1);

    let v1 = -wavelength_m / (4.0 * PI * t1) * r1.arg();
    let v2 = -wavelength_m / (4.0 * PI * t2) * r2.arg();
    (v1, v2)
}

/// Media de `x[i]·conj(x[i+1])` sobre los pares `(i, i+1)` con `i` de la
/// misma paridad que `start` (0: pares que empiezan en índice par; 1: los
/// que empiezan en índice impar).
fn mean_lag_product(x: &[Complex64], start: usize) -> Complex64 {
    let mut sum = Complex64::new(0.0, 0.0);
    let mut n = 0usize;
    let mut i = start;
    while i + 1 < x.len() {
        sum += x[i] * x[i + 1].conj();
        n += 1;
        i += 2;
    }
    assert!(
        n > 0,
        "no hay suficientes pares para estimar la autocovarianza"
    );
    sum / n as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_signal_has_zero_velocity() {
        let x = vec![Complex64::new(1.0, 0.0); 8];
        let (v1, v2) = staggered_pulse_pair_velocities(&x, 0.10, 0.8e-3, 1.2e-3);
        assert!(v1.abs() < 1e-9);
        assert!(v2.abs() < 1e-9);
    }
}
