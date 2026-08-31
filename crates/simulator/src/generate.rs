//! Generación de la serie temporal I/Q de una celda de rango, un canal
//! (método de Zrnić, 1975): conformado espectral de ruido blanco gaussiano
//! complejo seguido de IFFT, más ruido térmico aditivo.

use rand::Rng;
use rand_distr::{Distribution, StandardNormal};
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

use crate::spectrum::gaussian_doppler_spectrum;

/// Parámetros físicos de una celda de rango simulada.
pub struct CellParams {
    /// Potencia total de la señal meteorológica (unidades lineales arbitrarias;
    /// `E[|x|^2] == power_s` en la serie generada).
    pub power_s: f64,
    /// Velocidad radial media, m/s.
    pub mean_v: f64,
    /// Ancho espectral (desviación estándar de velocidad), m/s.
    pub sigma_v: f64,
    /// Longitud de onda del radar, m (no fijada por ningún plan: es un
    /// parámetro de entrada, ver suposición 2 del plan de implementación).
    pub wavelength_m: f64,
    /// Periodo entre pulsos, s.
    pub prt_s: f64,
    /// Número de pulsos (muestras) por celda.
    pub m: usize,
    /// Potencia de ruido térmico blanco aditivo, unidades lineales; 0.0 lo
    /// desactiva.
    pub noise_floor: f64,
}

/// Parámetros de la correlación cruzada H/V para la generación conjunta
/// (canal único no los usa). Ver `docs/algorithms/simulador-iq.md`
/// §"Variabilidad polarimétrica" — modo simultáneo (STAR).
pub struct DualPolParams {
    /// `10·log10(Zh/Zv)`, dB.
    pub zdr_db: f64,
    /// Módulo de la correlación copolar, en `[0,1]`.
    pub rho_hv: f64,
    /// Fase de la correlación copolar, grados.
    pub phidp_deg: f64,
}

fn complex_gaussian(rng: &mut impl Rng, variance: f64) -> Complex64 {
    // CN(0, variance): partes real e imaginaria independientes N(0, variance/2).
    let sigma = (variance / 2.0).sqrt();
    let re: f64 = StandardNormal.sample(rng);
    let im: f64 = StandardNormal.sample(rng);
    Complex64::new(re * sigma, im * sigma)
}

/// IFFT *sin renormalizar* + ruido térmico aditivo. La convención no
/// normalizada de `rustfft` (`y[n] = Σ_k X[k]·e^{j2πkn/M}`, sin `1/M`) es
/// exactamente la que hace que `E[|y[n]|²] == Σ_k |X[k]|²` sin factor de
/// escala adicional — ver la derivación en el test de autocovarianza, que lo
/// comprueba empíricamente.
fn shape_to_time_domain(
    rng: &mut impl Rng,
    mut spectral_samples: Vec<Complex64>,
    noise_floor: f64,
) -> Vec<Complex64> {
    let mut planner = FftPlanner::new();
    let ifft = planner.plan_fft_inverse(spectral_samples.len());
    ifft.process(&mut spectral_samples);

    if noise_floor > 0.0 {
        for x in spectral_samples.iter_mut() {
            *x += complex_gaussian(rng, noise_floor);
        }
    }

    spectral_samples
}

/// Genera `params.m` muestras complejas de una celda de rango, un canal.
pub fn generate_cell(params: &CellParams, rng: &mut impl Rng) -> Vec<Complex64> {
    let spectrum = gaussian_doppler_spectrum(
        params.power_s,
        params.mean_v,
        params.sigma_v,
        params.wavelength_m,
        params.prt_s,
        params.m,
    );

    let shaped: Vec<Complex64> = spectrum
        .iter()
        .map(|&s| complex_gaussian(rng, 1.0) * s.sqrt())
        .collect();

    shape_to_time_domain(rng, shaped, params.noise_floor)
}

/// Genera `params.m` muestras complejas de una celda de rango, dos canales
/// (H, V) conjuntamente correlacionados según `dual` — modo simultáneo
/// (STAR). Devuelve `(serie_h, serie_v)`.
///
/// El ruido blanco de cada bin se correlaciona *antes* del conformado
/// espectral por Cholesky de la matriz `[[1,ρ],[ρ*,1]]` con
/// `ρ = rho_hv·e^{jΦDP}` (`wh = w1`, `wv = ρ*·w1 + sqrt(1-ρ_hv²)·w2`, con
/// `w1,w2 ~ CN(0,1)` independientes — se comprueba `E[wh·v̄v] = ρ`). H y V
/// comparten la misma forma Doppler (`mean_v`, `sigma_v`); sólo cambia la
/// potencia total de V vía `zdr_db`. Como ambos espectros comparten la misma
/// forma normalizada, sumar la contribución de cada bin da exactamente
/// `E[x_h[n]·x̄_v[n]] = sqrt(power_s·power_v)·ρ` — el test de correlación
/// cruzada lo comprueba empíricamente. Reutiliza `params.noise_floor` para
/// ambos canales (misma cadena de recepción).
pub fn generate_dual_pol_cell(
    params: &CellParams,
    dual: &DualPolParams,
    rng: &mut impl Rng,
) -> (Vec<Complex64>, Vec<Complex64>) {
    assert!(
        (0.0..=1.0).contains(&dual.rho_hv),
        "rho_hv debe estar en [0,1]"
    );

    let power_v = params.power_s / 10f64.powf(dual.zdr_db / 10.0);
    let spectrum_h = gaussian_doppler_spectrum(
        params.power_s,
        params.mean_v,
        params.sigma_v,
        params.wavelength_m,
        params.prt_s,
        params.m,
    );
    let spectrum_v = gaussian_doppler_spectrum(
        power_v,
        params.mean_v,
        params.sigma_v,
        params.wavelength_m,
        params.prt_s,
        params.m,
    );

    let rho = Complex64::from_polar(dual.rho_hv, dual.phidp_deg.to_radians());
    let l21 = rho.conj();
    let l22 = (1.0 - dual.rho_hv * dual.rho_hv).sqrt();

    let mut shaped_h = Vec::with_capacity(params.m);
    let mut shaped_v = Vec::with_capacity(params.m);
    for k in 0..params.m {
        let w1 = complex_gaussian(rng, 1.0);
        let w2 = complex_gaussian(rng, 1.0);
        let wh = w1;
        let wv = l21 * w1 + l22 * w2;
        shaped_h.push(wh * spectrum_h[k].sqrt());
        shaped_v.push(wv * spectrum_v[k].sqrt());
    }

    let series_h = shape_to_time_domain(rng, shaped_h, params.noise_floor);
    let series_v = shape_to_time_domain(rng, shaped_v, params.noise_floor);
    (series_h, series_v)
}
