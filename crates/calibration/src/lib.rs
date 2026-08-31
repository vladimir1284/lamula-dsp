//! Cadena de calibración de reflectividad del LAMULA DSP
//! (`docs/algorithms/reflectivity-calibration.md`).
//!
//! El DSP **aplica** la calibración, no la determina: `radar_constant_db` es
//! un parámetro de `config` fijado por el operador tras un procedimiento de
//! banco/campo, no algo que este crate estime. Lo único que hay que calcular
//! en el camino de datos es la ecuación del radar meteorológico —constante
//! aditiva en dB más corrección por `r²`— aplicada a la potencia de señal ya
//! separada de ruido (`lamula_noise::subtract_noise`).
//!
//! `range_km` se mide contra una referencia de 1 km: `20·log10(range_km)` es
//! `20·log10(range_km / 1 km)`, así que `radar_constant_db` ya incorpora esa
//! referencia y no hace falta un parámetro `range_ref_km` aparte.

/// `Z [dBZ] = 10·log10(S) + 20·log10(range_km) + radar_constant_db`.
///
/// `s_linear` es la potencia de señal (ya restado el ruido, ver
/// `lamula_noise::subtract_noise`) en las mismas unidades lineales
/// arbitrarias que usa el resto del pipeline de momentos; debe ser positiva
/// — una celda sin señal detectable no tiene reflectividad que calcular, se
/// censura antes de llegar aquí.
pub fn power_to_dbz(s_linear: f64, range_km: f64, radar_constant_db: f64) -> f64 {
    assert!(
        s_linear > 0.0,
        "s_linear debe ser positivo (celda sin censurar)"
    );
    assert!(range_km > 0.0, "range_km debe ser positivo");
    10.0 * s_linear.log10() + 20.0 * range_km.log10() + radar_constant_db
}

/// Inversa de [`power_to_dbz`]: potencia lineal que produciría `dbz` a
/// `range_km` con la constante de radar dada. Uso principal: generar
/// escenarios de prueba con reflectividad de verdad-terreno conocida.
pub fn dbz_to_power(dbz: f64, range_km: f64, radar_constant_db: f64) -> f64 {
    assert!(range_km > 0.0, "range_km debe ser positivo");
    10f64.powf((dbz - radar_constant_db - 20.0 * range_km.log10()) / 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_to_dbz_and_back_round_trips() {
        let range_km = 75.0;
        let radar_constant_db = -40.0;
        for dbz in [-10.0, 0.0, 20.0, 45.0, 60.0] {
            let power = dbz_to_power(dbz, range_km, radar_constant_db);
            let back = power_to_dbz(power, range_km, radar_constant_db);
            assert!(
                (back - dbz).abs() < 1e-9,
                "round-trip falla para dbz={dbz}: recuperado={back}"
            );
        }
    }

    #[test]
    fn doubling_range_adds_about_6_db() {
        let radar_constant_db = -40.0;
        let power = 1.0;
        let near = power_to_dbz(power, 50.0, radar_constant_db);
        let far = power_to_dbz(power, 100.0, radar_constant_db);
        assert!(
            (far - near - 20.0 * 2f64.log10()).abs() < 1e-9,
            "duplicar el rango debe añadir 20*log10(2) ~= 6.02 dB: near={near} far={far}"
        );
    }
}
