//! Construye un `MomentRay` a partir de un radial ya ensamblado, igual que
//! `crates/rcp-link/tests/vertical_slice.rs` pero con los campos que ahí
//! eran valores fijos de prueba (`start_range_m`, `gate_spacing_m`,
//! `noise_floor_dbm`, `radar_constant_db`) tomados aquí del `config` real
//! aplicado a la sesión, no inventados.
//!
//! Momentos: UZ (sin corregir), CZ (corregida, `lamula_calibration`, y
//! filtrada de RFI/clutter cuando `rfi_filter`/`clutter_filter` están
//! activos — ver más abajo),
//! V, SQI y SIG del canal 0 — potencia/velocidad de UZ/CZ/V por pulse-pair o,
//! con `config.estimator = spectral`
//! (`docs/algorithms/estimador-espectral.md`), por periodograma
//! (`lamula_spectral::spectral_moments`); SQI/SIG siguen siendo pulse-pair
//! siempre, ver el doc-comment de [`gate_quality`] —, CCOR (`lamula_clutter`)
//! cuando hay filtro de clutter activo, más ZDR/ρHV/ΦDP/KDP cuando el radial trae un
//! segundo canal (`lamula_polarimetry` en modo simultáneo/STAR, `lamula_kdp`
//! sobre el ΦDP resultante) — ver el doc-comment de `crate` para por qué no
//! hay más. `acq_time_utc_ns`/`acq_monotonic_ns` usan el mismo
//! `timestamp_ns` del DRx para los dos campos: el contrato `DRx↔DSP` sólo
//! documenta ese campo como "instante del trigger, reloj del DRx", sin
//! confirmar que sea época UTC — la misma simplificación que ya hace el
//! vertical slice.
//!
//! Censura (`docs/algorithms/ruido-y-umbrales.md` §"Umbrales"): se evalúan
//! `sig_threshold`, `sqi_threshold` y `log_threshold` sobre cada celda; si
//! cualquiera de los tres dispara, UZ, CZ y V de esa celda se codifican
//! como NaN (`moment_flag::HAS_MISSING` en su bloque, `ray_flag::CENSORED`
//! en el radial) — "se censura el momento publicado, no la muestra de
//! entrada". SQI y SIG en sí NUNCA se censuran por estos umbrales: son el
//! índice que explica por qué la celda se descartó, y ocultarlo sería
//! contradecir la razón de publicarlo (misma página, última frase de esa
//! sección). SIG sí puede salir NaN cuando no está matemáticamente
//! definido (`s_linear <= 0`, ver `lamula_quality::sig_db`) — eso no es
//! censura por umbral, es un valor indefinido, y también marca
//! `HAS_MISSING`. CZ tiene una segunda fuente de NaN independiente de la
//! censura: `range_km <= 0` (`start_range_m == 0.0`, valor válido según el
//! contrato) hace que la ecuación del radar no tenga sentido para esa
//! celda — ver el comentario junto a `cz_values`.
//! Filtro de clutter (`lamula_clutter`, `docs/algorithms/gmap-clutter-filtering.md`):
//! GMAP o notch, según `clutter_filter`, sobre el periodograma con ventana
//! de Hann de la ráfaga cruda (`lamula_spectral::{hann_window,
//! periodogram_hann, bin_velocity}`, reutilizados sin reimplementar). Por la
//! semántica que fija `roadmap.md` §"Qué significa exactamente CZ" — UZ es
//! "sin filtrar", CZ es "tras el filtro de clutter" —, **sólo CZ** consume
//! la potencia corregida; UZ, V, SQI y SIG siguen viniendo del pulse-pair
//! crudo sin tocar, filtro activo o no. `CCOR` (`10·log10(potencia
//! cruda/potencia filtrada)`, vía `lamula_clutter::moments_from_spectrum`
//! sobre el espectro ya corregido) se publica sólo con filtro activo — sin
//! filtro no hay corrección que reportar, mismo criterio que los bloques
//! polarimétricos sin segundo canal. `ccor_threshold` censura CZ (NaN +
//! `HAS_MISSING`) cuando la corrección excede el umbral o es indefinida
//! (`ccor_db` sale `NaN` sin señal cruda detectable, o cuando el filtro no
//! deja nada por encima del ruido — este segundo caso, tratar "corrección
//! infinita" como indefinida en vez de publicarla, es inferencia mía sin
//! respaldo de oráculo). Esta censura es independiente de la de
//! SIG/SQI/LOG de arriba y, mismo criterio que la censura polarimétrica, no
//! marca `ray_flag::CENSORED` — sí marca `ray_flag::CLUTTER_FILTERED`
//! siempre que el filtro esté activo, y `moment_flag::FILTERED` en el
//! bloque de CZ.
//!
//! **Sin promediado**: la página del algoritmo documenta que el oráculo
//! exige promediar varios periodogramas independientes antes del ajuste de
//! GMAP — "un solo barrido es demasiado ruidoso bin a bin" — y deja esa
//! responsabilidad al llamador. Este pipeline no tiene con qué promediar
//! (una ráfaga por celda por radial, sin acumulación entre radiales ni
//! barridos): se corre sobre un único periodograma, autorizado
//! explícitamente por el usuario en vez de bloquear el cableo. El
//! comportamiento contrastado en `crates/clutter/tests/against_oracle.rs`
//! (recuperación bajo clutter fuerte, curva CSR, degradación acotada sin
//! clutter) se validó con `K_AVERAGES=10`, no con una sola realización — el
//! filtro corre, pero su varianza a un solo barrido no está contrastada
//! contra el oráculo.
//!
//! Filtro de RFI (`lamula_rfi`, `docs/algorithms/rfi-filtrado.md`): switch
//! independiente (`rfi_filter`), no supeditado a `clutter_filter`. Cuando
//! está activo corre sobre el mismo periodograma, ANTES del filtro de
//! clutter si los dos están activos — una línea de RFI dentro de la banda
//! de clutter distorsiona el ajuste gaussiano de GMAP (misma página,
//! "Interacción que hay que resolver explícitamente"). La interpolación
//! reutiliza `lamula_clutter::gmap_filter` con la máscara de
//! `lamula_rfi::detect_rfi_mask` en vez de reimplementar el relleno — "el
//! mismo mecanismo", según la propia página. Con RFI activo y clutter
//! inactivo, CZ sale corregida de RFI sin que se publique CCOR ni se marque
//! `ray_flag::CLUTTER_FILTERED` (literalmente "filtrado de clutter", no de
//! RFI, y el contrato v0.1 no tiene una bandera propia para RFI). Con los
//! dos activos, el CCOR publicado excluye la potencia que quitó RFI: se
//! calcula sobre la potencia YA sin RFI, no sobre la cruda, tal como exige
//! la página ("la potencia retirada por RFI no debe contarse en el
//! CCOR") — mezclar las dos haría que `ccor_threshold` censurara CZ por el
//! motivo equivocado. Igual que GMAP, corre sobre un único periodograma sin
//! promediar (mismo caveat de arriba); la calibración de
//! `DEFAULT_RFI_MEDIAN_DB`/`DEFAULT_RFI_WIDTH_MAX_BINS` es la del oráculo de
//! `lamula-rfi`, no repetida aquí.
//!
//! **Fragilidad numérica de RFI+clutter combinados, hallada al escribir el
//! test de este cableo (`rfi_and_clutter_together_still_publish_sane_ccor`)**:
//! con una banda de clutter angosta (pocos bins de `M`) el resultado del
//! ajuste gaussiano de GMAP puede depender de en qué bin discreto cae cada
//! frontera de la máscara — al punto de que una diferencia de redondeo
//! `f32→f64` de `wavelength_m` (el campo del contrato es `f32`; esta función
//! trabaja en `f64`) bastó para cambiar la corrección de clutter obtenida
//! sobre la MISMA ráfaga de prueba, de un CCOR positivo claro a
//! prácticamente cero. No es un bug de esta implementación: es evidencia de
//! que el diseño entero — un único periodograma sin promediar, ver "Sin
//! promediado" arriba — es más sensible de lo que las pruebas con clutter
//! solo (banda ancha, meteoro superpuesto de lleno) dejan ver. El test
//! correspondiente por eso sólo exige que el CCOR combinado salga definido
//! (no `NaN`), no un signo o magnitud concretos — exigir más ahí sería
//! afirmar una precisión que el diseño sin promediado no respalda. Repromediar
//! sobre varios barridos (la limitación real, no ésta) resolvería esto de
//! raíz al suavizar el periodograma antes del ajuste.
//!
//! ZDR/ρHV/ΦDP tienen su propia censura, independiente de la de arriba: si
//! `P_h` o `P_v` no superan `MIN_SNR_LIN_POLARIMETRIC` veces su propio ruido
//! (`lamula_polarimetry::PolarimetricFlag::Censored`), los tres salen NaN y
//! marcan `HAS_MISSING` en sus bloques — pero NO ponen `ray_flag::CENSORED`,
//! reservado a la censura de UZ/V de arriba (no hay una tercera cosa que
//! ese flag pueda distinguir). Si el radial sólo trae un canal, los tres
//! bloques simplemente no se publican aunque `moment_mask` los pida — no hay
//! canal V con que calcularlos.
//!
//! KDP (`lamula_kdp`, sólo con segundo canal, igual que ZDR/ρHV/ΦDP): se
//! calcula sobre el mismo perfil de ΦDP por celda, con dos pasos previos que
//! el propio crate delega en quien llama —
//! `docs/algorithms/kdp-estimacion.md` §"Parámetros del contrato que
//! consume"—: (1) censura adicional por ρHV bajo (`RHOHV_THRESHOLD_KDP`,
//! **sin respaldo documentado en este repo** — ver el comentario de esa
//! constante) y (2) desdoblado (`lamula_kdp::unwrap_deg`) antes del ajuste de
//! ventana deslizante (`lamula_kdp::kdp_window_fit`, `KDP_WINDOW_GATES`). Una
//! celda con KDP indeterminado (borde de perfil sin suficientes puntos, o
//! con algún ΦDP de entrada NaN dentro de su ventana) sale NaN +
//! `HAS_MISSING`, sin marcar `ray_flag::CENSORED` — mismo criterio que
//! ZDR/ρHV/ΦDP.
//!
//! Desdoblado de velocidad: dual-PRF (`lamula_dual_prf`, cross-radial vía
//! `PreviousPrf`) y staggered-PRT (`lamula_staggered_prt`, autocontenido
//! dentro del propio radial — ver [`staggered_velocity_mps`]) están
//! cableados. La conversión `Config` → `T1`,`T2` de staggered-PRT
//! (`staggered_prt_split`) es una inferencia mía sin respaldo de oráculo,
//! ver su doc-comment.
//!
//! Dealiasing de rango (`config.range_dealias`): sólo el nivel "detección y
//! marcado" está cableado, cross-radial vía `PreviousPrf` igual que
//! dual-PRF pero sin depender de `dealias_mode` — ver el doc-comment junto
//! a `range_dealias_detected` en [`build_moment_ray`], que también explica
//! por qué NO usa `classify_trip` de `lamula-range-dealias` (inferencia sin
//! respaldo de oráculo). La recuperación por fase aleatoria en magnetrón
//! (`lamula_range_dealias::recover_trip1`) sigue sin conectar: necesita fase
//! de burst por pulso, que el wire `DRx↔DSP` no transporta en ningún campo
//! (`crate::main` lo documenta), y el contrato tampoco tiene un campo de
//! hardware (magnetrón vs coherente) con que decidir si esa recuperación
//! aplicaría — ver "Decisiones cerradas" en `docs/algorithms/roadmap.md`.

use lamula_attenuation::zphi_correct_dbz;
use lamula_calibration::power_to_dbz;
use lamula_clutter::{gmap_filter, moments_from_spectrum, notch_filter};
use lamula_contract::dsp_rcp::{
    clutter_filter, data_type, dealias_mode, estimator, moment_flag, moment_kind, ray_flag,
    Config, MomentField, MomentRay,
};
use lamula_dual_prf::{continuity_fix, dealias_dual_prf};
use lamula_ingest::{ssi_counts_to_deg, AssembledRadial};
use lamula_kdp::{kdp_window_fit, unwrap_deg};
use lamula_moments::{pulse_pair_moments, PulsePairEstimate};
use lamula_noise::{censored_by_sig_threshold, snr_db};
use lamula_polarimetry::{polarimetric_moments_simultaneous, PolarimetricFlag};
use lamula_quality::{sig_db, sqi};
use lamula_rcp_link::wire::{MomentBlock, UpMessage};
use lamula_rfi::{detect_rfi_mask, DEFAULT_RFI_MEDIAN_DB, DEFAULT_RFI_WIDTH_MAX_BINS};
use lamula_spectral::{bin_velocity, hann_window, periodogram_hann, spectral_moments};
use lamula_staggered_prt::staggered_pulse_pair_velocities;
use rustfft::num_complex::Complex64;

const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// Margen mínimo de SNR lineal para censurar ZDR/ρHV/ΦDP
/// (`lamula_polarimetry::polarimetric_moments_simultaneous`). El valor de
/// referencia es el que usa `tools/oracles/polarimetria_covarianzas.ipynb`
/// (0.05) — `Config` (`contract/schema/dsp_rcp_v0_1.toml`) todavía no tiene
/// un campo propio para este margen, así que aquí queda fijo en vez de
/// inventarle un origen de configuración que no existe.
const MIN_SNR_LIN_POLARIMETRIC: f64 = 0.05;

/// Censura adicional de ΦDP por ρHV bajo antes de KDP
/// (`docs/algorithms/kdp-estimacion.md`, precondición del crate
/// `lamula-kdp`). **0.8 es una convención habitual en meteorología radar,
/// no un valor que este repositorio documente en ningún sitio** — ni la
/// página del algoritmo ni el oráculo (`tools/oracles/kdp_estimacion.ipynb`,
/// que barre ρHV 0.90/0.97/0.99 sin fijar un corte) dan una referencia
/// propia. Placeholder explícito hasta que exista un campo real en `Config`.
const RHOHV_THRESHOLD_KDP: f64 = 0.8;

/// Longitud de la ventana de ajuste de KDP, en celdas de rango. El propio
/// contrato v0.1 no tiene un campo para esto
/// (`docs/algorithms/kdp-estimacion.md` §"Parámetros del contrato que
/// consume": "hoy sería una constante de configuración local del DSP") — 15
/// celdas es el valor que usa el oráculo (`WINDOW_GATES`,
/// `tools/oracles/kdp_estimacion.ipynb`, ~2.25 km a 150 m de espaciado).
const KDP_WINDOW_GATES: usize = 15;

/// Radio de búsqueda de pliegues para `lamula_dual_prf::continuity_fix`. Sin
/// campo propio en `Config` (mismo motivo que `KDP_WINDOW_GATES`); 3 es el
/// valor del propio test de la función y del oráculo
/// (`tools/oracles/dual_prf_dealiasing.ipynb`, `continuity_fix`). Se
/// reutiliza tal cual para staggered-PRT (`staggered_velocity_mps`): mismo
/// mecanismo de continuidad espacial, sólo cambia si recorre radiales
/// (dual-PRF) o celdas de un mismo radial (staggered).
const DUAL_PRF_MAX_FOLD_SEARCH: i64 = 3;

/// Margen de selección de bins de señal para el ajuste gaussiano de GMAP
/// (`lamula_clutter::gmap_filter`, parámetro `margin`). Sin campo propio en
/// `Config` (mismo tipo de hueco que `RHOHV_THRESHOLD_KDP`); 3.0 es el valor
/// que usan los tres tests de contraste de
/// `crates/clutter/tests/against_oracle.rs` contra
/// `tools/oracles/gmap_clutter_filtering.ipynb`.
const GMAP_SIGNAL_MARGIN: f64 = 3.0;

/// Exponente `β` de la relación de acoplamiento atenuación-KDP que asume
/// `lamula_attenuation::zphi_correct_dbz` (`docs/algorithms/
/// atenuacion-zphi.md`) — NO es un campo de `Config`, mismo tipo de hueco
/// que `KDP_WINDOW_GATES`. 0.64884 es el valor que documenta Gu et al.
/// (2011) y usa Py-ART (`pyart.correct.calculate_attenuation_zphi`), común a
/// las tres bandas de su tabla de coeficientes — ver el doc-comment del
/// crate para el porqué (no hay acceso al paper original en este entorno).
const ZPHI_BETA: f64 = 0.64884;

/// Coeficiente de acoplamiento atenuación-fase `a_coef` [dB/grado] de
/// `lamula_attenuation::zphi_correct_dbz`, banda C (0.08, Gu et al. 2011 vía
/// Py-ART) — el contrato v0.1 no tiene un campo de banda del radar con que
/// elegir S/C/X automáticamente a partir de `wavelength_m` (mismo tipo de
/// hueco que `polarization_mode`, ver `docs/algorithms/roadmap.md`
/// §"Decisiones cerradas"); banda C es el valor por defecto declarado hasta
/// que exista ese campo o una constante de configuración local real.
const ZPHI_A_COEF_DB_PER_DEG: f64 = 0.08;

/// `(zdr, rhohv, phidp, kdp, phidp_unwrapped)` por celda, sólo con segundo
/// canal — ver el doc-comment del módulo. `phidp_unwrapped` (grados, la
/// misma serie que ya usa `kdp_window_fit`) se conserva aparte de `phidp`
/// (que sale módulo 360°, tal cual mide `lamula_polarimetry`) porque la
/// corrección de atenuación Z-PHI necesita el ΔΦDP de un tramo completo, no
/// el valor de una sola celda — ver su uso más abajo.
type PolarimetricValues = (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f64>);

/// Última medida de velocidad pulse-pair (sin desdoblar) y reflectividad
/// cruda (`uz_db`) de un radial, para emparejarla con el siguiente en modo
/// dual-PRF y en dealiasing de rango. `crate::main` la conserva entre
/// llamadas a [`build_moment_ray`]; se reinicia junto con el ensamblador en
/// `START`/`STOP`/`ENTER_SETUP` porque un emparejamiento con un radial de
/// antes de ese corte no tiene sentido físico.
///
/// `own_prt_s` es el PRT con el que se calculó `velocity_mps` en su momento
/// — no necesariamente el correcto: si ESE radial fue a su vez el primero
/// de una alternancia (rol desconocido), se calculó con `mean_prt_s`. Al
/// emparejarlo ahora, con el rol ya conocido por comparación de `prf_div`,
/// `velocity_mps` se reescala por `own_prt_s / prt_correcto` antes de
/// usarlo — la fase `arg(R1)` no depende de qué PRT se le haya aplicado
/// después, así que la velocidad medida escala linealmente con el PRT
/// asumido y esto recupera exactamente el valor que se habría obtenido con
/// el PRT correcto desde el principio, sin necesidad de guardar la fase
/// cruda.
///
/// `uz_db` no necesita reescalarse igual: es reflectividad, no fase, así
/// que no depende del PRT asumido — se usa tal cual como referencia sin
/// plegar para la detección de trip múltiple (ver el doc-comment del rango
/// de detección en [`build_moment_ray`]).
pub struct PreviousPrf {
    pub prf_div: u32,
    pub own_prt_s: f64,
    pub velocity_mps: Vec<f32>,
    pub uz_db: Vec<f32>,
}

/// `(prt_low_s, prt_high_s, v_a1, v_a2, v_ext)`: periodo y Nyquist de la PRF
/// baja/alta, más la Nyquist extendida dual-PRF, derivados sin conocer
/// `FS_HZ` del DRx (decisión D-02, ver el doc-comment de `crate`) a partir
/// de `prf_hz` —documentado como la *media* en dual-PRF— y
/// `prf_ratio_num`/`prf_ratio_den` (razón baja:alta). `k = 2·prf_hz /
/// (num+den)` da la unidad de la razón tal que `PRF_baja = num·k`,
/// `PRF_alta = den·k`; `v_ext = den·v_a1 == num·v_a2` por construcción
/// (comprobado contra el test de `lamula_dual_prf::dealias_dual_prf` en
/// `crates/dual-prf`, que usa una razón 2:3 y `v_ext = 3·v_a1`).
///
/// `prt_low_s`/`prt_high_s` importan tanto como `v_a1`/`v_a2`: el periodo
/// real de un radial en modo dual-PRF NO es `1/prf_hz` (la media) — usar la
/// media ahí escalaría mal la fase de `pulse_pair_moments` y produciría una
/// velocidad incorrecta, no simplemente plegada distinto. Ver el uso en
/// `build_moment_ray`.
fn dual_prf_split(config: &Config) -> (f64, f64, f64, f64, f64) {
    let wavelength_m = config.wavelength_m as f64;
    let num = config.prf_ratio_num as f64;
    let den = config.prf_ratio_den as f64;
    let k = 2.0 * config.prf_hz as f64 / (num + den);
    let prf_low = num * k;
    let prf_high = den * k;
    let v_a1 = wavelength_m * prf_low / 4.0;
    let v_a2 = wavelength_m * prf_high / 4.0;
    let v_ext = den * v_a1;
    (1.0 / prf_low, 1.0 / prf_high, v_a1, v_a2, v_ext)
}

/// `(t1_s, t2_s, v_a1, v_a2, v_ext)` para muestreo escalonado `T1,T2,T1,T2…`
/// (`docs/algorithms/staggered-prt.md` §"Parámetros del contrato que
/// consume"): reutiliza los mismos campos de `Config` que dual-PRF
/// (`prf_ratio_num`/`prf_ratio_den`, `prf_hz`), con el significado que la
/// propia página dice que fija el modo — aquí la razón es `T1:T2`, no
/// PRF baja:alta. **Inferencia mía, no contrastada**: el doc del campo en
/// el schema (`contract/schema/dsp_rcp_v0_1.toml`) dice literalmente "razón
/// dual-PRF", sin mencionar staggered, y ningún oráculo
/// (`tools/oracles/staggered_prt.ipynb`) muestra la conversión
/// `prf_hz`+razón → `T1`,`T2` en segundos — el oráculo y
/// `crates/staggered-prt/tests/against_oracle.rs` fijan `T1`/`T2` como
/// constantes directas, nunca derivadas de un `prf_hz`. Asumo aquí que
/// `prf_hz` sigue significando "media de las dos PRF resultantes (1/T1,
/// 1/T2)", mismo criterio que ya uso en [`dual_prf_split`] — autorizado
/// explícitamente por el usuario en vez de bloquear el cableo.
///
/// De `T = (num+den) / (2·num·den·prf_hz)`: `T1 = num·T`, `T2 = den·T` (si
/// `num < den`, `T1 < T2`). `v_ext` usa el periodo diferencia `|T2−T1|`, tal
/// como describe la página del algoritmo.
fn staggered_prt_split(config: &Config) -> (f64, f64, f64, f64, f64) {
    let wavelength_m = config.wavelength_m as f64;
    let num = config.prf_ratio_num as f64;
    let den = config.prf_ratio_den as f64;
    let t = (num + den) / (2.0 * num * den * config.prf_hz as f64);
    let t1 = num * t;
    let t2 = den * t;
    let v_a1 = wavelength_m / (4.0 * t1);
    let v_a2 = wavelength_m / (4.0 * t2);
    let v_ext = wavelength_m / (4.0 * (t2 - t1).abs());
    (t1, t2, v_a1, v_a2, v_ext)
}

/// Velocidad desdoblada por celda en modo staggered-PRT: a diferencia de
/// dual-PRF, las dos medidas plegadas (`v1`, `v2`) salen de la MISMA ráfaga
/// —no hace falta radial anterior ni `PreviousPrf`—, así que nunca hay
/// "sin con qué emparejar" y `ray_flag::DEALIAS_FAILED` no se marca en este
/// modo (misma razón que en dual-PRF: no hay umbral de residuo aceptable
/// documentado para marcarlo por convergencia). Reutiliza
/// `lamula_dual_prf::dealias_dual_prf`/`continuity_fix` sin reimplementar
/// el teorema chino del resto — `docs/algorithms/staggered-prt.md` lo
/// llama "exactamente el mismo mecanismo".
fn staggered_velocity_mps(radial: &AssembledRadial, config: &Config) -> Vec<f32> {
    let wavelength_m = config.wavelength_m as f64;
    let (t1, t2, v_a1, v_a2, v_ext) = staggered_prt_split(config);
    // `dealias_dual_prf`/`continuity_fix` buscan pliegues del PRIMER eje que
    // reciben (pasos de `2·v_a`), y usan el segundo sólo para reconciliar
    // sin desdoblarlo: por diseño el primer eje tiene que ser el de Nyquist
    // más CHICA (periodo más largo). Si se pasa al revés, la ventana de
    // aceptación (`v_ext + v_a_eje1`) se ensancha lo suficiente para
    // admitir un candidato fantasma exactamente a `4·v_a_eje1 == 3·(2·v_a_
    // eje2)` de distancia — coincidencia EXACTA por la razón simple
    // `T1:T2`, no un caso raro de redondeo (así se descubrió: test de este
    // módulo con `v_true` cerca del borde de `v_ext` fallaba en seco). No
    // se asume cuál de `T1`/`T2` es la más larga; se decide comparando
    // `v_a1`/`v_a2` directamente.
    let t1_is_finer_axis = v_a1 <= v_a2;
    let (axis1_va, axis2_va) = if t1_is_finer_axis {
        (v_a1, v_a2)
    } else {
        (v_a2, v_a1)
    };

    let v_hats: Vec<f64> = radial.channels[0]
        .iter()
        .map(|series| {
            let (v_from_t1, v_from_t2) =
                staggered_pulse_pair_velocities(series, wavelength_m, t1, t2);
            let (axis1_meas, axis2_meas) = if t1_is_finer_axis {
                (v_from_t1, v_from_t2)
            } else {
                (v_from_t2, v_from_t1)
            };
            dealias_dual_prf(axis1_meas, axis2_meas, axis1_va, axis2_va, v_ext).velocity_mps
        })
        .collect();
    continuity_fix(&v_hats, axis1_va, DUAL_PRF_MAX_FOLD_SEARCH)
        .into_iter()
        .map(|v| v as f32)
        .collect()
}

/// Cantidades derivadas de un `PulsePairEstimate` que la censura y los
/// cuatro momentos publicados necesitan, calculadas una sola vez por celda.
struct GateQuality {
    uz_db: f64,
    sqi_value: Option<f64>,
    sig_value: Option<f64>,
    censored: bool,
}

/// `power_linear` es la potencia del momento a publicar (`docs/algorithms/
/// estimador-espectral.md` §"Parámetros del contrato que consume": con
/// `estimator = spectral` es `SpectralEstimate::power_linear`, no
/// `e.s_linear`, para que el umbral `log_threshold` decida sobre la misma
/// potencia que UZ/CZ terminan publicando — ver el doc-comment junto al
/// cálculo de `primary_power_linear`/`primary_velocity_mps` en
/// [`build_moment_ray`]). SQI y SIG siguen atados a `e` (autocovarianza
/// pulse-pair) porque `lamula_quality::sqi`/`sig_db` no tienen definición
/// espectral en este repo — **inferencia mía sin respaldo de oráculo**: ni
/// la página del estimador espectral ni su oráculo dicen qué pasa con SQI
/// cuando `estimator = spectral`, sólo que "los umbrales de censura actúan
/// igual que con el estimador primario".
fn gate_quality(power_linear: f64, e: &PulsePairEstimate, config: &Config) -> GateQuality {
    let uz_db = if power_linear > 0.0 {
        10.0 * power_linear.log10()
    } else {
        f64::NEG_INFINITY
    };
    let snr = if e.s_linear > 0.0 {
        snr_db(e.s_linear, e.noise_floor_estimate)
    } else {
        f64::NEG_INFINITY
    };
    // `sqi()` exige r0_raw > 0; sólo falla con una ráfaga exactamente cero,
    // que en datos reales no ocurre (siempre hay algo de ruido del
    // receptor) — se guarda igual para no entrar en pánico ante ese caso
    // degenerado.
    let sqi_value = (e.r0_raw > 0.0).then(|| sqi(e.r0_raw, e.r1_abs));
    let sig_value = sig_db(e.s_linear, e.noise_floor_estimate);

    let censored = censored_by_sig_threshold(snr, config.sig_threshold as f64)
        || sqi_value.map_or(true, |v| v < config.sqi_threshold as f64)
        || uz_db <= config.log_threshold as f64;

    GateQuality {
        uz_db,
        sqi_value,
        sig_value,
        censored,
    }
}

/// Potencia corregida por el filtro de clutter y `CCOR` para una celda —
/// ver el doc-comment del módulo, sección "Filtro de clutter".
struct ClutterResult {
    filtered_power_linear: f64,
    ccor_db: f64,
}

/// Cablea RFI/GMAP/notch sobre la ráfaga cruda de una celda: periodograma
/// con ventana de Hann (`lamula_spectral`, reutilizado sin reimplementar),
/// RFI primero si `rfi_filter_enabled` (`lamula_rfi::detect_rfi_mask` +
/// `gmap_filter` reutilizado como relleno — ver el doc-comment del módulo),
/// después clutter con máscara centrada en v=0 de anchura `clutter_width_mps`
/// — pese al nombre del campo del contrato (`clutter_width_ms`), la unidad
/// documentada en el esquema es m/s, mismo tipo de nombre heredado que
/// `prf_ratio_num` en [`staggered_prt_split`] — y extracción de momentos
/// sobre el espectro corregido (`lamula_clutter::moments_from_spectrum`).
/// `noise_floor_estimate` es el mismo `PulsePairEstimate::noise_floor_estimate`
/// ya calculado por `pulse_pair_moments` sobre esta misma ráfaga (HS74 en el
/// dominio de potencia total, `R(0)`); dividido entre `M` da el umbral por
/// bin que pide `gmap_filter`/`moments_from_spectrum`, sin repetir la
/// estimación.
#[allow(clippy::too_many_arguments)]
fn clutter_filtered_power(
    series: &[Complex64],
    raw_s_linear: f64,
    noise_floor_estimate: f64,
    wavelength_m: f64,
    prt_s: f64,
    clutter_width_mps: f64,
    filter_mode: u8,
    rfi_filter_enabled: bool,
) -> ClutterResult {
    let m = series.len();
    let win = hann_window(m);
    let p = periodogram_hann(series, &win);
    let v_a = wavelength_m / (4.0 * prt_s);
    let bin_spacing = 2.0 * v_a / m as f64;
    let v_k: Vec<f64> = (0..m)
        .map(|k| bin_velocity(k, m, wavelength_m, prt_s))
        .collect();
    let n_thresh = noise_floor_estimate / m as f64;

    // RFI antes que clutter (docs/algorithms/rfi-filtrado.md §"Interacción
    // que hay que resolver explícitamente"). `ccor_numerator` es la
    // potencia de referencia para el CCOR de clutter: sin RFI activo, la
    // cruda de siempre (`raw_s_linear`, pulse-pair); con RFI activo, la
    // potencia YA sin RFI, para que su corrección no se cuente en el CCOR
    // de clutter.
    let (p_for_clutter, ccor_numerator) = if rfi_filter_enabled {
        let rfi_mask = detect_rfi_mask(&p, DEFAULT_RFI_MEDIAN_DB, DEFAULT_RFI_WIDTH_MAX_BINS);
        let no_rfi = gmap_filter(&p, &v_k, &rfi_mask, n_thresh, GMAP_SIGNAL_MARGIN).filtered;
        let post_rfi_power =
            moments_from_spectrum(&no_rfi, &v_k, bin_spacing, n_thresh).power_linear;
        (no_rfi, post_rfi_power)
    } else {
        (p, raw_s_linear)
    };

    let clutter_active = filter_mode != clutter_filter::NONE;
    let filtered_p = if !clutter_active {
        p_for_clutter.clone()
    } else {
        let mask: Vec<bool> = v_k
            .iter()
            .map(|&v| v.abs() <= clutter_width_mps / 2.0)
            .collect();
        if filter_mode == clutter_filter::NOTCH {
            notch_filter(&p_for_clutter, &mask)
        } else {
            gmap_filter(&p_for_clutter, &v_k, &mask, n_thresh, GMAP_SIGNAL_MARGIN).filtered
        }
    };
    let filtered_power_linear =
        moments_from_spectrum(&filtered_p, &v_k, bin_spacing, n_thresh).power_linear;

    // CCOR sólo tiene sentido con clutter activo -- es por definición "la
    // corrección atribuible al clutter" (roadmap.md); con RFI activo y
    // clutter no, no hay CCOR que publicar (ver el doc-comment del módulo).
    // `NaN` también cuando no está definido con clutter activo: sin señal de
    // referencia detectable (mismo criterio que `sig_db`), o cuando el
    // filtro no deja nada por encima del ruido -- tratar esa "corrección
    // infinita" como indefinida en vez de publicarla es inferencia mía.
    let ccor_db = if clutter_active && ccor_numerator > 0.0 && filtered_power_linear > 0.0 {
        10.0 * (ccor_numerator / filtered_power_linear).log10()
    } else {
        f64::NAN
    };

    ClutterResult {
        filtered_power_linear,
        ccor_db,
    }
}

pub fn build_moment_ray(
    radial: &AssembledRadial,
    config: &Config,
    seq: u32,
    first_after_config: bool,
    ssi_counts_per_turn: u32,
    ssi_zero_offset_deg: f64,
    previous_prf: Option<&PreviousPrf>,
) -> (UpMessage, PreviousPrf) {
    let wavelength_m = config.wavelength_m as f64;
    let mean_prt_s = 1.0 / config.prf_hz as f64;

    // Dual-PRF: `radial.prf_div` sólo dice "distinto o igual que el
    // anterior" (D-02: sin `FS_HZ` no hay Hz absoluto que sacarle) — el rol
    // de ESTE radial (PRF baja o alta del par) sólo se conoce comparándolo
    // con el radial anterior. Sin ese punto de referencia (primer radial
    // tras `START`/config, o dos radiales seguidos con el mismo `prf_div` —
    // el DRx no alternó como se le pidió) no hay forma de saber a qué PRT
    // real corresponden sus pulsos: usar la media (`mean_prt_s`) ahí sería
    // adivinar la escala de fase, no sólo perder el desdoblado — por eso
    // ese caso también marca `ray_flag::DEALIAS_FAILED`, aunque sea el
    // *periodo* lo que falla y no sólo el pliegue.
    let mut dealias_failed = false;
    let dual_prf_role: Option<bool> = if config.dealias_mode == dealias_mode::DUAL_PRF {
        match previous_prf {
            // `true` = este radial es el de PRF baja (mayor `prf_div`).
            Some(prev) if prev.prf_div != radial.prf_div => {
                Some(radial.prf_div > prev.prf_div)
            }
            _ => {
                dealias_failed = true;
                None
            }
        }
    } else {
        None
    };
    let own_prt_s = match dual_prf_role {
        Some(is_low) => {
            let (prt_low, prt_high, ..) = dual_prf_split(config);
            if is_low {
                prt_low
            } else {
                prt_high
            }
        }
        None => mean_prt_s,
    };

    // UZ/V/SQI/SIG sólo corren sobre el canal 0 — H, por convención del
    // contrato. El canal 1 (V), cuando está presente, sólo alimenta
    // ZDR/ρHV/ΦDP/KDP más abajo. El pulse-pair se calcula SIEMPRE, incluso
    // con `estimator = spectral`: `r0_raw`/`r1_abs`/`noise_floor_estimate`
    // siguen alimentando SQI/SIG (ver [`gate_quality`]) y el umbral de
    // ruido del filtro de clutter (`clutter_filtered_power`), ninguno de
    // los dos con equivalente espectral documentado.
    let estimates: Vec<_> = radial.channels[0]
        .iter()
        .map(|series| pulse_pair_moments(series, wavelength_m, own_prt_s))
        .collect();
    let n_gates = estimates.len() as u16;

    // Estimador primario de potencia/velocidad para UZ/CZ/V
    // (`docs/algorithms/estimador-espectral.md` §"Parámetros del contrato
    // que consume": `estimator = spectral` lo selecciona). Con
    // `PULSE_PAIR` esto es exactamente `estimates` reempaquetado; con
    // `SPECTRAL` se corre `spectral_moments` sobre la misma ráfaga cruda.
    // Cuando el periodograma no encuentra línea principal por encima del
    // umbral de ruido (`SpectralFlag::Censored`, `velocity_mps: None`) se
    // recae en la velocidad de fase del pulse-pair para esa celda en vez de
    // un valor centinela — mismo criterio de degradación que ya usa
    // `PulsePairEstimate` cuando `S <= 0` (la fase se calcula igual, sin
    // pretender que tenga sentido físico) — **inferencia mía sin respaldo
    // de oráculo**: ni la página del algoritmo ni su oráculo cubren la
    // interacción con una celda censurada.
    let (primary_power_linear, primary_velocity_mps): (Vec<f64>, Vec<f64>) =
        if config.estimator == estimator::SPECTRAL {
            radial.channels[0]
                .iter()
                .zip(estimates.iter())
                .map(|(series, pp)| {
                    let se = spectral_moments(series, wavelength_m, own_prt_s);
                    (se.power_linear, se.velocity_mps.unwrap_or(pp.velocity_mps))
                })
                .unzip()
        } else {
            (
                estimates.iter().map(|e| e.s_linear).collect(),
                estimates.iter().map(|e| e.velocity_mps).collect(),
            )
        };

    let quality: Vec<GateQuality> = primary_power_linear
        .iter()
        .zip(estimates.iter())
        .map(|(&p, e)| gate_quality(p, e, config))
        .collect();
    let any_censored = quality.iter().any(|q| q.censored);

    // Filtro de RFI/clutter (GMAP/notch): sólo se corre cuando alguno de los
    // dos está activo — es la etapa más cara del pipeline (una FFT por
    // celda, ver `docs/algorithms/gmap-clutter-filtering.md` §"Coste de
    // cómputo") — y sólo afecta a CZ (ver el doc-comment del módulo).
    // `own_prt_s` es el mismo PRT ya resuelto arriba para el pulse-pair de
    // este radial; en modo staggered-PRT eso es `mean_prt_s`
    // (`dual_prf_role` es `None` fuera de `DUAL_PRF`), una aproximación no
    // contrastada contra ningún oráculo para esa combinación.
    let rfi_filter_enabled = config.rfi_filter != 0;
    let clutter_results: Option<Vec<ClutterResult>> = (config.clutter_filter
        != clutter_filter::NONE
        || rfi_filter_enabled)
        .then(|| {
            radial.channels[0]
                .iter()
                .zip(estimates.iter())
                .map(|(series, e)| {
                    clutter_filtered_power(
                        series,
                        e.s_linear,
                        e.noise_floor_estimate,
                        wavelength_m,
                        own_prt_s,
                        config.clutter_width_ms as f64,
                        config.clutter_filter,
                        rfi_filter_enabled,
                    )
                })
                .collect()
        });

    let mut uz_values: Vec<f32> = quality
        .iter()
        .map(|q| if q.censored { f32::NAN } else { q.uz_db as f32 })
        .collect();
    let raw_velocity_mps: Vec<f32> = primary_velocity_mps.iter().map(|&v| v as f32).collect();

    // Con el rol conocido y el radial anterior del mismo número de celdas,
    // se reconcilian las dos medidas (cada una ya escalada con su propio
    // PRT real, ver arriba) y se aplica continuidad espacial. Sin eso, se
    // publica la velocidad tal cual (ya con la escala correcta si el rol se
    // conoció, aunque no se haya podido desdoblar). La convergencia por
    // celda (residuo de `dealias_dual_prf`) no se evalúa aparte: no hay
    // umbral documentado en este repo para "residuo aceptable", mismo tipo
    // de hueco que `RHOHV_THRESHOLD_KDP`.
    // Staggered-PRT no toca `dual_prf_role`/`previous_prf` en absoluto: el
    // desdoblado es autocontenido dentro del propio radial (ver
    // `staggered_velocity_mps`). `raw_velocity_mps` (calculada arriba con
    // `mean_prt_s`, sin sentido físico como fase de un único retardo
    // uniforme en este modo) no se usa como velocidad publicada aquí, sólo
    // queda en `PreviousPrf` por uniformidad de la firma — dual-PRF nunca
    // la lee porque `dual_prf_role` es `None` cuando `dealias_mode !=
    // DUAL_PRF`.
    let dealiased_velocity_mps: Vec<f32> = if config.dealias_mode == dealias_mode::STAGGERED_PRT {
        staggered_velocity_mps(radial, config)
    } else {
        match (dual_prf_role, previous_prf) {
        (Some(is_low), Some(prev)) if prev.velocity_mps.len() == raw_velocity_mps.len() => {
            let (prt_low, prt_high, v_a1, v_a2, v_ext) = dual_prf_split(config);
            // El rol de `prev` es el opuesto del de este radial (si no,
            // `prev.prf_div == radial.prf_div` y no se habría llegado aquí
            // — ver `dual_prf_role`). Se reescala su velocidad guardada al
            // PRT correcto de SU rol antes de usarla: puede haberse
            // calculado con `mean_prt_s` si `prev` fue a su vez un radial
            // aislado (ver el doc-comment de `PreviousPrf`).
            let prev_correct_prt_s = if is_low { prt_high } else { prt_low };
            let prev_velocity_rescaled: Vec<f64> = prev
                .velocity_mps
                .iter()
                .map(|&v| v as f64 * (prev.own_prt_s / prev_correct_prt_s))
                .collect();
            let v_hats: Vec<f64> = if is_low {
                raw_velocity_mps
                    .iter()
                    .zip(&prev_velocity_rescaled)
                    .map(|(&v1, &v2)| dealias_dual_prf(v1 as f64, v2, v_a1, v_a2, v_ext).velocity_mps)
                    .collect()
            } else {
                prev_velocity_rescaled
                    .iter()
                    .zip(&raw_velocity_mps)
                    .map(|(&v1, &v2)| dealias_dual_prf(v1, v2 as f64, v_a1, v_a2, v_ext).velocity_mps)
                    .collect()
            };
            continuity_fix(&v_hats, v_a1, DUAL_PRF_MAX_FOLD_SEARCH)
                .into_iter()
                .map(|v| v as f32)
                .collect()
        }
        (Some(_), Some(_)) => {
            // Rol conocido pero número de celdas distinto del radial
            // anterior: no se puede emparejar celda a celda.
            dealias_failed = true;
            raw_velocity_mps.clone()
        }
        _ => raw_velocity_mps.clone(),
        }
    };

    let mut v_values: Vec<f32> = dealiased_velocity_mps
        .iter()
        .zip(&quality)
        .map(|(&v, q)| if q.censored { f32::NAN } else { v })
        .collect();
    // SQI y SIG nunca se censuran por umbral: ver el doc-comment del
    // módulo. Sólo salen NaN cuando la cantidad no está definida
    // (`Option::None` de `gate_quality`).
    let sqi_values: Vec<f32> = quality
        .iter()
        .map(|q| q.sqi_value.map(|v| v as f32).unwrap_or(f32::NAN))
        .collect();
    let sig_values: Vec<f32> = quality
        .iter()
        .map(|q| q.sig_value.map(|v| v as f32).unwrap_or(f32::NAN))
        .collect();

    // CZ (`lamula_calibration::power_to_dbz`): misma censura que UZ/V (ver
    // arriba, `q.censored` ya implica `primary_power_linear[i] > 0` —
    // `gate_quality` fuerza `uz_db = NEG_INFINITY <= log_threshold` en ese
    // caso, así que nunca llega sin censurar una potencia no positiva a
    // `power_to_dbz`, que entra en pánico con eso). Guardia aparte:
    // `range_km <= 0.0` (`start_range_m == 0.0`, valor válido según
    // `lamula_rcp_link::validate::validate_config`, sólo lo rechaza si es
    // negativo) — la ecuación del radar no tiene sentido a rango cero, así
    // que esa celda sale NaN aunque no esté censurada por umbral. Sin
    // filtro de clutter/RFI activo, la potencia de referencia es
    // `primary_power_linear[i]` (pulse-pair o espectral según
    // `config.estimator`, ver arriba); con filtro activo, el propio
    // `clutter_filtered_power` ya resuelve en el dominio espectral
    // independientemente del estimador primario, así que su resultado no
    // cambia con `config.estimator`.
    let radar_constant_db = config.radar_constant_db as f64;
    let start_range_km = config.start_range_m as f64 / 1000.0;
    let gate_spacing_km_cz = config.gate_spacing_m as f64 / 1000.0;
    let mut cz_values: Vec<f32> = quality
        .iter()
        .zip(primary_power_linear.iter())
        .enumerate()
        .map(|(i, (q, &pl))| {
            let range_km = start_range_km + i as f64 * gate_spacing_km_cz;
            if q.censored || range_km <= 0.0 {
                return f32::NAN;
            }
            match &clutter_results {
                // Corrección excesiva o indefinida (ver
                // `clutter_filtered_power`): censura propia de CZ,
                // independiente de `q.censored` y sin marcar
                // `ray_flag::CENSORED` — mismo criterio que la censura
                // polarimétrica de arriba. Sólo aplica con clutter activo:
                // el CCOR (y su umbral) no está definido cuando lo único
                // activo es RFI — ver el doc-comment del módulo.
                Some(results) => {
                    let r = &results[i];
                    let ccor_over_threshold = config.clutter_filter != clutter_filter::NONE
                        && (r.ccor_db.is_nan() || r.ccor_db > config.ccor_threshold as f64);
                    if ccor_over_threshold {
                        f32::NAN
                    } else {
                        power_to_dbz(r.filtered_power_linear, range_km, radar_constant_db) as f32
                    }
                }
                None => power_to_dbz(pl, range_km, radar_constant_db) as f32,
            }
        })
        .collect();

    // Dealiasing de rango, nivel "detección y marcado"
    // (`docs/algorithms/dealiasing-de-rango.md` §"Cómo funciona", primer
    // peldaño, disponible en toda instalación). El rol PRF alta/baja se
    // deriva de `radial.prf_div` con el mismo criterio que `dual_prf_role`
    // arriba, pero SIN pasar por `dealias_mode`: la vía práctica que la
    // página describe ("comparar el mismo azimut con dos PRFs distintas, lo
    // que los modos de corte ya proporcionan") no depende de que el
    // desdoblado de VELOCIDAD esté en modo dual-PRF, sólo de que el barrido
    // alterne PRF radial a radial — de ahí el cálculo aparte en vez de
    // reutilizar `dual_prf_role`.
    //
    // **Inferencia sin respaldo de oráculo** (mismo tipo de hueco que
    // `staggered_prt_split`, ver su doc-comment). Ni `classify_trip` de
    // `lamula-range-dealias` ni su oráculo (`tools/oracles/
    // dealiasing_de_rango.ipynb`, "Prueba 1") cubren este caso: los dos
    // modelan un blanco puntual con posición aparente medida, no la malla
    // de celdas de eco distribuido que produce un radial meteorológico real
    // — es literalmente el hueco que documenta `crate::main` para explicar
    // por qué `classify_trip` no está conectado ("no incluye la detección
    // de picos que haría falta para usarlo de verdad"). En vez de eso, para
    // cada celda `n` de un radial de PRF alta con eco detectable
    // (`uz_values[n]` no censurado), se compara la MISMA celda `n` del
    // radial de PRF baja anterior (hipótesis trip1: misma posición física,
    // sin plegar) contra su celda `n + fold_gates` (hipótesis trip2:
    // posición física `r_n + r_max_alta`, `fold_gates = round(r_max_alta /
    // gate_spacing_m)`). Si la celda de PRF baja NO tiene eco en la
    // posición trip1 pero SÍ lo tiene en la posición trip2, esta celda se
    // marca contaminada — NaN en UZ/CZ/V, `ray_flag::CENSORED` (el
    // vocabulario que la página señala). Cualquier otra combinación,
    // incluida la falta de referencia (radial aislado, o `n + fold_gates`
    // fuera del radial de PRF baja), se deja como trip1: mismo default
    // conservador que usa `classify_trip` cuando `apparent_low_prf_m` es
    // `None` — "sin referencia no hay base para acusar solapamiento". No
    // hay curva de aceptación frente a SNR contrastada para este criterio;
    // el oráculo sólo la da para su modelo de blanco puntual.
    let mut range_dealias_detected = false;
    if config.range_dealias != 0 {
        let range_dealias_role: Option<bool> = match previous_prf {
            Some(prev) if prev.prf_div != radial.prf_div => Some(radial.prf_div > prev.prf_div),
            _ => None,
        };
        if let (Some(false), Some(prev)) = (range_dealias_role, previous_prf) {
            // `Some(false)`: este radial es el de PRF alta (menor
            // `prf_div`, mismo criterio que `dual_prf_role`); `prev` es el
            // de PRF baja, referencia sin plegar. Los radiales de PRF baja
            // no se tocan aquí — su propio `r_max` ya cubre estas celdas
            // sin ambigüedad, no hay nada que reconciliar.
            let (_, prt_high, ..) = dual_prf_split(config);
            let r_max_high_m = SPEED_OF_LIGHT_M_S * prt_high / 2.0;
            let fold_gates = (r_max_high_m / config.gate_spacing_m as f64).round() as isize;
            for n in 0..uz_values.len() {
                if uz_values[n].is_nan() {
                    continue; // sin eco propio, nada que atribuir
                }
                let trip1_has_echo = prev.uz_db.get(n).is_some_and(|v| v.is_finite());
                if trip1_has_echo {
                    continue; // referencia trip1 disponible: default conservador
                }
                let trip2_has_echo = usize::try_from(n as isize + fold_gates)
                    .ok()
                    .and_then(|i| prev.uz_db.get(i))
                    .is_some_and(|v| v.is_finite());
                if trip2_has_echo {
                    uz_values[n] = f32::NAN;
                    v_values[n] = f32::NAN;
                    cz_values[n] = f32::NAN;
                    range_dealias_detected = true;
                }
            }
        }
    }
    // Snapshot para `PreviousPrf`: tomado ya con la censura de arriba
    // aplicada (si corrió), antes de que `uz_values` pueda moverse al
    // bloque UZ más abajo.
    let uz_db_for_next = uz_values.clone();

    // Sólo hay ρHV/ZDR/ΦDP/KDP con un segundo canal (V) presente en el
    // radial — `channel_mask`/`channels.len()` lo refleja tal cual llega
    // del DRx.
    let polarimetric: Option<PolarimetricValues> = (radial.channels.len() > 1)
        .then(|| {
            let mut zdr = Vec::with_capacity(n_gates as usize);
            let mut rhohv = Vec::with_capacity(n_gates as usize);
            let mut phidp = Vec::with_capacity(n_gates as usize);
            for (h, v) in radial.channels[0].iter().zip(radial.channels[1].iter()) {
                let est = polarimetric_moments_simultaneous(
                    h,
                    v,
                    config.zdr_offset_db as f64,
                    config.phidp_offset_deg as f64,
                    MIN_SNR_LIN_POLARIMETRIC,
                );
                let (z, r, p) = match est.flag {
                    PolarimetricFlag::Ok => {
                        (est.zdr_db as f32, est.rhohv as f32, est.phidp_deg as f32)
                    }
                    PolarimetricFlag::Censored => (f32::NAN, f32::NAN, f32::NAN),
                };
                zdr.push(z);
                rhohv.push(r);
                phidp.push(p);
            }

            // KDP: censura adicional por ρHV bajo, luego desdoblado y ajuste
            // de ventana sobre el ΦDP resultante — ver el doc-comment del
            // módulo para las dos constantes sin campo propio en `Config`.
            let phidp_for_kdp: Vec<f64> = phidp
                .iter()
                .zip(&rhohv)
                .map(|(&p, &r)| {
                    if r.is_nan() || (r as f64) < RHOHV_THRESHOLD_KDP {
                        f64::NAN
                    } else {
                        p as f64
                    }
                })
                .collect();
            let phidp_unwrapped = unwrap_deg(&phidp_for_kdp);
            let gate_spacing_km = config.gate_spacing_m as f64 / 1000.0;
            let kdp: Vec<f32> = kdp_window_fit(&phidp_unwrapped, gate_spacing_km, KDP_WINDOW_GATES)
                .into_iter()
                .map(|k| k.map(|v| v as f32).unwrap_or(f32::NAN))
                .collect();

            (zdr, rhohv, phidp, kdp, phidp_unwrapped)
        });

    // Corrección de atenuación Z-PHI sobre CZ (Testud et al. 2000, ver
    // `lamula_attenuation`) — el alcance nuevo que `docs/algorithms/
    // roadmap.md` §"Decisiones cerradas" ("Qué significa exactamente CZ")
    // decide agregar a su significado, sólo alcanzable con segundo canal
    // (ΦDP). Se aplica sobre cada tramo contiguo MAXIMAL donde tanto
    // `cz_values` como `phidp_unwrapped` están definidos: `phidp_unwrapped`
    // puede tener más de un tramo así en un mismo rayo si hay más de un
    // parche de lluvia separado por celdas censuradas (una celda `NaN` de
    // `unwrap_deg` no corrompe el desdoblado de las celdas válidas
    // siguientes, sólo dentro del hueco mismo — ver su doc-comment). Tramos
    // de una sola celda (sin intervalo que integrar, `zphi_correct_dbz`
    // exige al menos dos) se dejan sin corregir. **Inferencia mía sin
    // respaldo de oráculo**: ni la página del algoritmo ni el oráculo
    // (`tools/oracles/atenuacion_zphi.ipynb`) cubren la interacción con
    // rayos que combinan varios parches de lluvia y huecos censurados; el
    // criterio de segmentación es el más simple que respeta "censura, no
    // corrige" sobre lo que no se puede medir con confianza.
    if let Some((_, _, _, _, phidp_unwrapped)) = &polarimetric {
        let gate_spacing_km = config.gate_spacing_m as f64 / 1000.0;
        let mut run_start: Option<usize> = None;
        for i in 0..=cz_values.len() {
            let valid =
                i < cz_values.len() && !cz_values[i].is_nan() && phidp_unwrapped[i].is_finite();
            if valid {
                run_start.get_or_insert(i);
                continue;
            }
            if let Some(start) = run_start.take() {
                if i - start >= 2 {
                    let z_dbz: Vec<f64> = cz_values[start..i].iter().map(|&v| v as f64).collect();
                    let delta_phidp = phidp_unwrapped[i - 1] - phidp_unwrapped[start];
                    let corrected = zphi_correct_dbz(
                        &z_dbz,
                        gate_spacing_km,
                        ZPHI_BETA,
                        ZPHI_A_COEF_DB_PER_DEG,
                        delta_phidp,
                    );
                    for (dst, v) in cz_values[start..i].iter_mut().zip(corrected) {
                        *dst = v as f32;
                    }
                }
            }
        }
    }

    let az_start_deg =
        ssi_counts_to_deg(radial.azimuth_raw, ssi_counts_per_turn, ssi_zero_offset_deg) as f32;
    let el_start_deg = ssi_counts_to_deg(
        radial.elevation_raw,
        ssi_counts_per_turn,
        ssi_zero_offset_deg,
    ) as f32;

    let mut ray_flags = 0u8;
    if first_after_config {
        ray_flags |= ray_flag::FIRST_AFTER_CONFIG;
    }
    if any_censored || range_dealias_detected {
        ray_flags |= ray_flag::CENSORED;
    }
    if dealias_failed {
        ray_flags |= ray_flag::DEALIAS_FAILED;
    }
    if config.clutter_filter != clutter_filter::NONE {
        ray_flags |= ray_flag::CLUTTER_FILTERED;
    }

    let moment_flags = |values: &[f32]| -> u8 {
        if values.iter().any(|v| v.is_nan()) {
            moment_flag::HAS_MISSING
        } else {
            0
        }
    };

    let mut moments = Vec::with_capacity(10);
    if config.moment_mask & (1 << moment_kind::UZ) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::UZ,
                data_type: data_type::F32,
                flags: moment_flags(&uz_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: uz_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::V) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::V,
                data_type: data_type::F32,
                flags: moment_flags(&v_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: v_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::SQI) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::SQI,
                data_type: data_type::F32,
                flags: moment_flags(&sqi_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: sqi_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::SIG) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::SIG,
                data_type: data_type::F32,
                flags: moment_flags(&sig_values),
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: sig_values,
        });
    }
    if config.moment_mask & (1 << moment_kind::CZ) != 0 {
        moments.push(MomentBlock {
            field: MomentField {
                kind: moment_kind::CZ,
                data_type: data_type::F32,
                // `CORRECTED` va siempre, no sólo cuando el bloque no tiene
                // NaN: describe que ESTE momento (a diferencia de UZ) lleva
                // la ecuación del radar aplicada, no el estado de censura
                // de una celda en particular — y, con segundo canal
                // presente, también la corrección de atenuación Z-PHI (ver
                // el bloque que mutó `cz_values` más arriba y el
                // doc-comment de `PolarimetricValues`); no hay una bandera
                // propia en el contrato v0.1 para distinguir "corregida sólo
                // por ecuación del radar" de "corregida además de
                // atenuación", así que ambos casos publican `CORRECTED`.
                // `FILTERED` igual, cuando el filtro de clutter está activo
                // (único bloque afectado, ver el doc-comment del módulo).
                flags: moment_flags(&cz_values)
                    | moment_flag::CORRECTED
                    | if clutter_results.is_some() {
                        moment_flag::FILTERED
                    } else {
                        0
                    },
                pad0: 0,
                n_gates: n_gates as u32,
                scale: 1.0,
                offset: 0.0,
            },
            values: cz_values,
        });
    }
    if let Some(results) = &clutter_results {
        if config.clutter_filter != clutter_filter::NONE
            && config.moment_mask & (1 << moment_kind::CCOR) != 0
        {
            let ccor_values: Vec<f32> = results.iter().map(|r| r.ccor_db as f32).collect();
            moments.push(MomentBlock {
                field: MomentField {
                    kind: moment_kind::CCOR,
                    data_type: data_type::F32,
                    flags: moment_flags(&ccor_values),
                    pad0: 0,
                    n_gates: n_gates as u32,
                    scale: 1.0,
                    offset: 0.0,
                },
                values: ccor_values,
            });
        }
    }
    if let Some((zdr_values, rhohv_values, phidp_values, kdp_values, _)) = polarimetric {
        if config.moment_mask & (1 << moment_kind::ZDR) != 0 {
            moments.push(MomentBlock {
                field: MomentField {
                    kind: moment_kind::ZDR,
                    data_type: data_type::F32,
                    flags: moment_flags(&zdr_values),
                    pad0: 0,
                    n_gates: n_gates as u32,
                    scale: 1.0,
                    offset: 0.0,
                },
                values: zdr_values,
            });
        }
        if config.moment_mask & (1 << moment_kind::RHOHV) != 0 {
            moments.push(MomentBlock {
                field: MomentField {
                    kind: moment_kind::RHOHV,
                    data_type: data_type::F32,
                    flags: moment_flags(&rhohv_values),
                    pad0: 0,
                    n_gates: n_gates as u32,
                    scale: 1.0,
                    offset: 0.0,
                },
                values: rhohv_values,
            });
        }
        if config.moment_mask & (1 << moment_kind::PHIDP) != 0 {
            moments.push(MomentBlock {
                field: MomentField {
                    kind: moment_kind::PHIDP,
                    data_type: data_type::F32,
                    flags: moment_flags(&phidp_values),
                    pad0: 0,
                    n_gates: n_gates as u32,
                    scale: 1.0,
                    offset: 0.0,
                },
                values: phidp_values,
            });
        }
        if config.moment_mask & (1 << moment_kind::KDP) != 0 {
            moments.push(MomentBlock {
                field: MomentField {
                    kind: moment_kind::KDP,
                    data_type: data_type::F32,
                    flags: moment_flags(&kdp_values),
                    pad0: 0,
                    n_gates: n_gates as u32,
                    scale: 1.0,
                    offset: 0.0,
                },
                values: kdp_values,
            });
        }
    }

    let ray = MomentRay {
        seq,
        acq_time_utc_ns: radial.timestamp_ns_start,
        acq_monotonic_ns: radial.timestamp_ns_start,
        // Sin controlador de antena en este repo: un solo radial estático
        // por barrido/volumen, todos con índice 0.
        volume_seq: 0,
        sweep_seq: 0,
        ray_index: 0,
        n_gates,
        n_pulses: config.n_pulses,
        bins_valid: n_gates,
        n_moments: moments.len() as u8,
        sweep_mode: config.sweep_mode,
        prf_mode: config.dealias_mode,
        ray_flags,
        pad0: 0,
        az_start_deg,
        az_end_deg: az_start_deg,
        el_start_deg,
        el_end_deg: el_start_deg,
        fixed_angle_deg: el_start_deg,
        start_range_m: config.start_range_m,
        gate_spacing_m: config.gate_spacing_m,
        prf_hz: config.prf_hz,
        // Dual-PRF y staggered-PRT publican la Nyquist EXTENDIDA (doc de
        // cada algoritmo, §"Parámetros del contrato que consume"), no la de
        // un solo periodo.
        nyquist_velocity: match config.dealias_mode {
            dealias_mode::DUAL_PRF => dual_prf_split(config).4 as f32,
            dealias_mode::STAGGERED_PRT => staggered_prt_split(config).4 as f32,
            _ => (wavelength_m / (4.0 * mean_prt_s)) as f32,
        },
        unambiguous_range_m: (SPEED_OF_LIGHT_M_S / (2.0 * config.prf_hz as f64)) as f32,
        noise_floor_dbm: config.noise_floor_dbm,
        radar_constant_db: config.radar_constant_db,
    };

    let previous_prf_out = PreviousPrf {
        prf_div: radial.prf_div,
        own_prt_s,
        velocity_mps: raw_velocity_mps,
        uz_db: uz_db_for_next,
    };

    (UpMessage::MomentRay { ray, moments }, previous_prf_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamula_moments::PulsePairFlag;

    fn config_with_thresholds(
        sig_threshold: f32,
        sqi_threshold: f32,
        log_threshold: f32,
    ) -> Config {
        Config {
            seq: 1,
            moment_mask: 0,
            n_pulses: 64,
            n_gates: 1,
            clutter_filter: 0,
            dealias_mode: 0,
            sweep_mode: 0,
            estimator: 0,
            rfi_filter: 0,
            range_dealias: 0,
            prf_ratio_num: 0,
            prf_ratio_den: 0,
            start_range_m: 0.0,
            gate_spacing_m: 250.0,
            prf_hz: 1000.0,
            sqi_threshold,
            sig_threshold,
            ccor_threshold: 20.0,
            log_threshold,
            clutter_width_ms: 1.0,
            radar_constant_db: 65.0,
            noise_floor_dbm: -108.0,
            receiver_gain_db: 40.0,
            zdr_offset_db: 0.0,
            phidp_offset_deg: 0.0,
            wavelength_m: 0.10,
            polarization_mode: 0,
            pad0: 0,
            pad1: 0,
        }
    }

    fn estimate(
        s_linear: f64,
        r0_raw: f64,
        r1_abs: f64,
        noise_floor_estimate: f64,
    ) -> PulsePairEstimate {
        PulsePairEstimate {
            s_linear,
            r0_raw,
            r1_abs,
            noise_floor_estimate,
            velocity_mps: 3.0,
            spectrum_width_mps: Some(1.0),
            flag: if s_linear > 0.0 {
                PulsePairFlag::Ok
            } else {
                PulsePairFlag::Censored
            },
        }
    }

    #[test]
    fn strong_coherent_signal_is_not_censored() {
        // S=1.0, N=0.01 -> SNR=20dB; r1_abs cerca de r0_raw -> SQI alto;
        // uz_db = 0dB, muy por encima de log_threshold.
        let e = estimate(1.0, 1.01, 0.95, 0.01);
        let config = config_with_thresholds(3.0, 0.4, -10.0);
        let q = gate_quality(e.s_linear, &e, &config);
        assert!(!q.censored);
        assert!(q.sqi_value.unwrap() > 0.4);
        assert!(q.sig_value.unwrap() > 3.0);
    }

    #[test]
    fn low_snr_censors_but_still_publishes_sig() {
        // S=0.02, N=0.01 -> SNR=3.01dB, muy por debajo del umbral (10dB).
        let e = estimate(0.02, 1.03, 0.95, 0.01);
        let config = config_with_thresholds(10.0, 0.0, -100.0);
        let q = gate_quality(e.s_linear, &e, &config);
        assert!(q.censored, "SNR bajo umbral debería censurar UZ/V");
        assert!(
            q.sig_value.is_some(),
            "SIG no se censura por umbral: sigue publicado aunque censure UZ/V"
        );
    }

    #[test]
    fn low_sqi_censors_even_with_good_snr() {
        // SNR alto (S=1.0, N=0.01) pero r1_abs pequeño frente a r0_raw ->
        // SQI bajo: censura por coherencia, no por SNR.
        let e = estimate(1.0, 1.01, 0.05, 0.01);
        let config = config_with_thresholds(3.0, 0.4, -100.0);
        let q = gate_quality(e.s_linear, &e, &config);
        assert!(q.sqi_value.unwrap() < 0.4);
        assert!(q.censored, "SQI bajo umbral debería censurar UZ/V");
        assert!(q.sig_value.is_some(), "SIG sigue publicado");
    }

    #[test]
    fn cell_with_no_detectable_signal_has_undefined_sig() {
        let e = estimate(0.0, 0.01, 0.005, 0.01);
        let config = config_with_thresholds(3.0, 0.4, -10.0);
        let q = gate_quality(e.s_linear, &e, &config);
        assert!(q.censored);
        assert!(
            q.sig_value.is_none(),
            "sin señal detectable SIG no está definido, no es 'censurado'"
        );
    }

    use lamula_simulator::{generate_cell, generate_dual_pol_cell, CellParams, DualPolParams};
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use rustfft::num_complex::Complex64;

    fn polarimetric_config(moment_mask: u32) -> Config {
        Config {
            moment_mask,
            zdr_offset_db: 0.0,
            phidp_offset_deg: 0.0,
            ..config_with_thresholds(3.0, 0.4, -100.0)
        }
    }

    fn radial_from_channels(channels: Vec<Vec<Vec<Complex64>>>) -> AssembledRadial {
        AssembledRadial {
            seq_start: 1,
            timestamp_ns_start: 0,
            trigger_count_start: 0,
            azimuth_raw: 0,
            elevation_raw: 0,
            prf_div: 1,
            pulse_width_idx: 0,
            pulse_mode: 0,
            cell_mode: 0,
            channel_mask: if channels.len() > 1 { 0b0011 } else { 0b0001 },
            channels,
            dropped_pulses: 0,
        }
    }

    const ALL_MOMENTS_MASK: u32 = (1 << moment_kind::UZ)
        | (1 << moment_kind::CZ)
        | (1 << moment_kind::V)
        | (1 << moment_kind::SQI)
        | (1 << moment_kind::SIG)
        | (1 << moment_kind::ZDR)
        | (1 << moment_kind::RHOHV)
        | (1 << moment_kind::PHIDP)
        | (1 << moment_kind::KDP);

    #[test]
    fn dual_channel_radial_publishes_polarimetric_moments() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.0 / 1000.0,
            m: 64,
            noise_floor: 0.01,
        };
        let dual = DualPolParams {
            zdr_db: 2.0,
            rho_hv: 0.98,
            phidp_deg: 10.0,
        };
        let (h, v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
        let radial = radial_from_channels(vec![vec![h], vec![v]]);
        let config = polarimetric_config(ALL_MOMENTS_MASK);

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };

        for kind in [moment_kind::ZDR, moment_kind::RHOHV, moment_kind::PHIDP] {
            let block = moments
                .iter()
                .find(|m| m.field.kind == kind)
                .unwrap_or_else(|| panic!("falta el bloque de moment_kind {kind}"));
            assert!(
                block.values[0].is_finite(),
                "celda con SNR y coherencia altas no debería censurarse"
            );
        }
        // Un solo gate no llena ni el mínimo de 3 puntos de la ventana de
        // KDP: indeterminado, no censurado, pero igual sale NaN.
        let kdp_block = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::KDP)
            .expect("falta el bloque de KDP");
        assert!(kdp_block.values[0].is_nan());
    }

    #[test]
    fn single_channel_radial_omits_polarimetric_moments() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.0 / 1000.0,
            m: 64,
            noise_floor: 0.01,
        };
        let dual = DualPolParams {
            zdr_db: 2.0,
            rho_hv: 0.98,
            phidp_deg: 10.0,
        };
        let (h, _v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
        let radial = radial_from_channels(vec![vec![h]]);
        let config = polarimetric_config(ALL_MOMENTS_MASK);

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };

        for kind in [
            moment_kind::ZDR,
            moment_kind::RHOHV,
            moment_kind::PHIDP,
            moment_kind::KDP,
        ] {
            assert!(
                !moments.iter().any(|m| m.field.kind == kind),
                "sin canal V no hay con qué calcular moment_kind {kind}"
            );
        }
    }

    #[test]
    fn decorrelated_channels_censor_polarimetric_moments() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.0 / 1000.0,
            m: 64,
            noise_floor: 0.01,
        };
        // rho_hv = 0.0 no basta por sí solo para forzar la censura (que
        // depende del margen de SNR por canal, no de rho_hv): se baja
        // `power_s` muy por debajo del ruido para que sí dispare (con
        // margen amplio frente al ruido de estimación de `noise_floor`).
        let cell = CellParams {
            power_s: 0.0001,
            ..cell
        };
        let dual = DualPolParams {
            zdr_db: 0.0,
            rho_hv: 0.0,
            phidp_deg: 0.0,
        };
        let (h, v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
        let radial = radial_from_channels(vec![vec![h], vec![v]]);
        let config = polarimetric_config(ALL_MOMENTS_MASK);

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };

        for kind in [
            moment_kind::ZDR,
            moment_kind::RHOHV,
            moment_kind::PHIDP,
            moment_kind::KDP,
        ] {
            let block = moments
                .iter()
                .find(|m| m.field.kind == kind)
                .unwrap_or_else(|| panic!("falta el bloque de moment_kind {kind}"));
            assert!(
                block.values[0].is_nan(),
                "SNR por debajo del margen debería censurar ZDR/ρHV/ΦDP (y arrastrar KDP)"
            );
            assert_eq!(block.field.flags, moment_flag::HAS_MISSING);
        }
    }

    #[test]
    fn kdp_window_fit_recovers_slope_from_dual_channel_radial() {
        // Perfil de ΦDP lineal (KDP verdadero constante), igual que el test
        // de `lamula_kdp::kdp_window_fit`, pero generado a través de IQ
        // dual-pol real por celda en vez de un array analítico — comprueba
        // que el cableo (censura por ρHV, `unwrap_deg`, `kdp_window_fit`)
        // reconstruye una pendiente conocida, no la exactitud del algoritmo
        // en sí (eso ya lo cubre `crates/kdp/tests/against_oracle.rs`).
        const K0_DEG_PER_KM: f64 = 2.0;
        const GATE_SPACING_KM: f64 = 0.150;
        const N_GATES: usize = 30;

        let mut rng = StdRng::seed_from_u64(20260901);
        let mut h_channel = Vec::with_capacity(N_GATES);
        let mut v_channel = Vec::with_capacity(N_GATES);
        for i in 0..N_GATES {
            let cell = CellParams {
                power_s: 1.0,
                mean_v: 3.0,
                sigma_v: 1.0,
                wavelength_m: 0.10,
                prt_s: 1.0 / 1000.0,
                m: 64,
                noise_floor: 0.001,
            };
            let dual = DualPolParams {
                zdr_db: 0.0,
                rho_hv: 0.999,
                phidp_deg: 2.0 * K0_DEG_PER_KM * i as f64 * GATE_SPACING_KM,
            };
            let (h, v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
            h_channel.push(h);
            v_channel.push(v);
        }
        let radial = radial_from_channels(vec![h_channel, v_channel]);
        let mut config = polarimetric_config(ALL_MOMENTS_MASK);
        config.gate_spacing_m = (GATE_SPACING_KM * 1000.0) as f32;

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };

        let kdp_block = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::KDP)
            .expect("falta el bloque de KDP");
        // Celda central: ventana completa de 15, lejos de los dos bordes.
        let mid = kdp_block.values[N_GATES / 2];
        assert!(
            (mid as f64 - K0_DEG_PER_KM).abs() < 1.0,
            "KDP recuperado ({mid}) debería acercarse a K0={K0_DEG_PER_KM} deg/km"
        );
    }

    #[test]
    fn zphi_correction_recovers_attenuated_cz_on_dual_channel_radial() {
        // Perfil de Z verdadero constante (30 dBZ) atenuado con A verdadero
        // constante (perfil de Z constante -> atenuación constante en el
        // modelo Z-A acoplado) y ΔΦDP acorde a `ZPHI_A_COEF_DB_PER_DEG`,
        // generado a través de IQ dual-pol real por celda -- misma técnica
        // que `kdp_window_fit_recovers_slope_from_dual_channel_radial`, pero
        // con potencia variable en vez de constante para que la ecuación del
        // radar por sí sola NO explique el CZ recuperado.
        use lamula_calibration::dbz_to_power;

        const GATE_SPACING_KM: f64 = 0.150;
        const N_GATES: usize = 40;
        const RADAR_CONSTANT_DB: f64 = -20.0;
        const START_RANGE_KM: f64 = 5.0;
        const Z_TRUE_DBZ: f64 = 30.0;
        const A_TRUE_DB_PER_KM: f64 = 0.3;

        let kdp_true_deg_per_km = A_TRUE_DB_PER_KM / ZPHI_A_COEF_DB_PER_DEG;
        let far = N_GATES - 5;
        let uncorrected_bias_db = 2.0 * A_TRUE_DB_PER_KM * far as f64 * GATE_SPACING_KM;
        assert!(
            uncorrected_bias_db > 2.0,
            "escenario de prueba debería tener atenuación significativa sin corregir: {uncorrected_bias_db} dB"
        );

        // Promediado sobre varias realizaciones (mismo criterio que
        // `crates/calibration/tests/against_oracle.rs`): una sola ráfaga por
        // celda deja bastante ruido de potencia y de fase como para que el
        // sesgo de UNA realización no sea representativo del método.
        const N_TRIALS: usize = 15;
        let mut rng = StdRng::seed_from_u64(20260902);
        let mut recovered_sum = 0.0;
        for _ in 0..N_TRIALS {
            let mut h_channel = Vec::with_capacity(N_GATES);
            let mut v_channel = Vec::with_capacity(N_GATES);
            for i in 0..N_GATES {
                let range_km = START_RANGE_KM + i as f64 * GATE_SPACING_KM;
                let two_way_atten_db = 2.0 * A_TRUE_DB_PER_KM * i as f64 * GATE_SPACING_KM;
                let power_s =
                    dbz_to_power(Z_TRUE_DBZ - two_way_atten_db, range_km, RADAR_CONSTANT_DB);
                let cell = CellParams {
                    power_s,
                    mean_v: 3.0,
                    sigma_v: 1.0,
                    wavelength_m: 0.10,
                    prt_s: 1.0 / 1000.0,
                    m: 128,
                    noise_floor: 0.001,
                };
                let dual = DualPolParams {
                    zdr_db: 0.0,
                    rho_hv: 0.999,
                    phidp_deg: 2.0 * kdp_true_deg_per_km * i as f64 * GATE_SPACING_KM,
                };
                let (h, v) = generate_dual_pol_cell(&cell, &dual, &mut rng);
                h_channel.push(h);
                v_channel.push(v);
            }
            let radial = radial_from_channels(vec![h_channel, v_channel]);
            let mut config = polarimetric_config(ALL_MOMENTS_MASK);
            config.gate_spacing_m = (GATE_SPACING_KM * 1000.0) as f32;
            config.start_range_m = (START_RANGE_KM * 1000.0) as f32;
            config.radar_constant_db = RADAR_CONSTANT_DB as f32;

            let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
            let UpMessage::MomentRay { moments, .. } = msg else {
                panic!("se esperaba MomentRay");
            };
            let cz_block = moments
                .iter()
                .find(|m| m.field.kind == moment_kind::CZ)
                .expect("falta el bloque de CZ");
            recovered_sum += cz_block.values[far] as f64;
        }
        let recovered_mean = recovered_sum / N_TRIALS as f64;

        // Celda lejana: atenuación acumulada sin corregir de varios dB --
        // comprueba que la corrección, no sólo la ecuación del radar,
        // recuperó el valor verdadero.
        assert!(
            (recovered_mean - Z_TRUE_DBZ).abs() < 1.5,
            "CZ corregido ({recovered_mean:.3}) debería acercarse a Z verdadero ({Z_TRUE_DBZ}); el sesgo sin corregir hubiera sido {uncorrected_bias_db} dB"
        );
    }

    fn dual_prf_config() -> Config {
        // Misma razón 2:3 que el test de `lamula_dual_prf::dealias_dual_prf`
        // (`PRT1=1.2ms`, `PRT2=0.8ms`): PRF1≈833.33Hz, PRF2=1250Hz.
        let mean_prf_hz = (1.0 / 1.2e-3 + 1.0 / 0.8e-3) / 2.0;
        Config {
            dealias_mode: dealias_mode::DUAL_PRF,
            prf_hz: mean_prf_hz as f32,
            prf_ratio_num: 2,
            prf_ratio_den: 3,
            moment_mask: (1 << moment_kind::UZ) | (1 << moment_kind::V),
            ..config_with_thresholds(3.0, 0.4, -100.0)
        }
    }

    #[test]
    fn isolated_dual_prf_radial_marks_dealias_failed() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 25.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.2e-3, // PRF baja del par (833.33 Hz)
            m: 64,
            noise_floor: 0.01,
        };
        let h = generate_cell(&cell, &mut rng);
        let mut radial = radial_from_channels(vec![vec![h]]);
        radial.prf_div = 3; // mayor divisor = PRF baja

        let (msg, _) = build_moment_ray(&radial, &dual_prf_config(), 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { ray, .. } = msg else {
            panic!("se esperaba MomentRay");
        };
        assert_eq!(
            ray.ray_flags & ray_flag::DEALIAS_FAILED,
            ray_flag::DEALIAS_FAILED,
            "sin radial anterior no hay con qué emparejar"
        );
    }

    #[test]
    fn dual_prf_pair_unfolds_velocity_across_two_radials() {
        // Reproduce el escenario del test de `lamula_dual_prf::dealias_dual_prf`
        // (v_true=25 m/s, V_A1≈20.83, V_A2≈31.25) pero generando IQ real por
        // radial en vez de invocar `dealias_dual_prf` directo — comprueba el
        // cableo completo: PRT real por rol (no la media), emparejamiento por
        // `prf_div` y desdoblado.
        const V_TRUE: f64 = 25.0;
        let config = dual_prf_config();

        let mut rng = StdRng::seed_from_u64(20260901);
        let cell_low = CellParams {
            power_s: 1.0,
            mean_v: V_TRUE,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.2e-3, // PRF baja (833.33 Hz)
            m: 64,
            noise_floor: 0.01,
        };
        let h1 = generate_cell(&cell_low, &mut rng);
        let mut radial1 = radial_from_channels(vec![vec![h1]]);
        radial1.prf_div = 3; // mayor divisor = PRF baja

        let cell_high = CellParams {
            prt_s: 0.8e-3, // PRF alta (1250 Hz)
            ..cell_low
        };
        let h2 = generate_cell(&cell_high, &mut rng);
        let mut radial2 = radial_from_channels(vec![vec![h2]]);
        radial2.prf_div = 2; // menor divisor = PRF alta

        let (_, previous_prf) =
            build_moment_ray(&radial1, &config, 1, false, 1_000_000, 0.0, None);
        let (msg2, _) = build_moment_ray(
            &radial2,
            &config,
            2,
            false,
            1_000_000,
            0.0,
            Some(&previous_prf),
        );
        let UpMessage::MomentRay { ray, moments } = msg2 else {
            panic!("se esperaba MomentRay");
        };

        assert_eq!(
            ray.ray_flags & ray_flag::DEALIAS_FAILED,
            0,
            "segundo radial del par sí debería poder desdoblar"
        );
        let v_block = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::V)
            .expect("falta el bloque de V");
        assert!(
            (v_block.values[0] as f64 - V_TRUE).abs() < 2.0,
            "V desdoblado ({}) debería acercarse a v_true={V_TRUE}",
            v_block.values[0]
        );
    }

    fn generate_channel(cells: &[CellParams], rng: &mut StdRng) -> Vec<Vec<Complex64>> {
        cells.iter().map(|c| generate_cell(c, rng)).collect()
    }

    #[test]
    fn range_dealias_detection_censors_only_trip2_evidenced_cells() {
        // Reutiliza el par de PRT de `dual_prf_config` (1.2ms/0.8ms, razón
        // 2:3) pero con `dealias_mode = NONE` y `range_dealias = 1`: prueba
        // que la detección de trip múltiple corre independiente del modo de
        // desdoblado de velocidad, tal como describe el doc-comment junto a
        // `range_dealias_detected` en `build_moment_ray`.
        let mut config = dual_prf_config();
        config.dealias_mode = dealias_mode::NONE;
        config.range_dealias = 1;
        config.moment_mask = 1 << moment_kind::UZ;

        let (_, prt_high, ..) = dual_prf_split(&config);
        let r_max_high_m = SPEED_OF_LIGHT_M_S * prt_high / 2.0;
        config.gate_spacing_m = (r_max_high_m / 3.0) as f32; // fold_gates = 3

        let echo = CellParams {
            power_s: 1.0,
            mean_v: 0.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.2e-3,
            m: 64,
            noise_floor: 0.01,
        };
        let noise = CellParams {
            power_s: 0.0,
            ..echo
        };

        let mut rng = StdRng::seed_from_u64(20260902);

        // Radial de PRF baja (prf_div=3, PRT=1.2ms): referencia sin plegar.
        // idx0=eco (trip1 de la celda 0 de PRF alta); idx1/idx2=ruido (sin
        // trip1 para las celdas 1/2); idx3=ruido (sin uso directo);
        // idx4=eco (trip2 de la celda 1); idx5=ruido (sin trip2 para la
        // celda 2).
        let low_cells = [
            CellParams { prt_s: 1.2e-3, ..echo },
            CellParams { prt_s: 1.2e-3, ..noise },
            CellParams { prt_s: 1.2e-3, ..noise },
            CellParams { prt_s: 1.2e-3, ..noise },
            CellParams { prt_s: 1.2e-3, ..echo },
            CellParams { prt_s: 1.2e-3, ..noise },
        ];
        let low_channel = generate_channel(&low_cells, &mut rng);
        let mut radial_low = radial_from_channels(vec![low_channel]);
        radial_low.prf_div = 3;

        // Radial de PRF alta (prf_div=2, PRT=0.8ms): celdas 0-2 con eco
        // propio; celda 3 sin eco (control: no debe evaluarse pese a no
        // tener referencia).
        let high_cells = [
            CellParams { prt_s: 0.8e-3, ..echo },
            CellParams { prt_s: 0.8e-3, ..echo },
            CellParams { prt_s: 0.8e-3, ..echo },
            CellParams { prt_s: 0.8e-3, ..noise },
        ];
        let high_channel = generate_channel(&high_cells, &mut rng);
        let mut radial_high = radial_from_channels(vec![high_channel]);
        radial_high.prf_div = 2;

        let (_, previous_prf) =
            build_moment_ray(&radial_low, &config, 1, false, 1_000_000, 0.0, None);
        let (msg, _) = build_moment_ray(
            &radial_high,
            &config,
            2,
            false,
            1_000_000,
            0.0,
            Some(&previous_prf),
        );
        let UpMessage::MomentRay { ray, moments } = msg else {
            panic!("se esperaba MomentRay");
        };
        let uz = &moments
            .iter()
            .find(|m| m.field.kind == moment_kind::UZ)
            .expect("falta el bloque de UZ")
            .values;

        assert!(
            uz[0].is_finite(),
            "celda 0: eco en la posición trip1, no debería censurarse"
        );
        assert!(
            uz[1].is_nan(),
            "celda 1: sin eco en trip1 pero con eco en trip2, debería censurarse"
        );
        assert!(
            uz[2].is_finite(),
            "celda 2: sin referencia en ninguna hipótesis, default conservador trip1"
        );
        assert_eq!(
            ray.ray_flags & ray_flag::CENSORED,
            ray_flag::CENSORED,
            "detección de trip2 debería marcar ray_flag::CENSORED"
        );
    }

    fn staggered_prt_config() -> Config {
        // Misma razón 2:3 que `crates/staggered-prt/tests/against_oracle.rs`
        // (T1=0.8ms, T2=1.2ms): con esa razón, `staggered_prt_split` da
        // exactamente esos T1/T2 a partir del mismo `prf_hz` medio que ya
        // usa `dual_prf_config` (coincide en valor porque es el mismo par
        // T1/T2, sólo con los roles PRF baja/alta invertidos).
        let mean_prf_hz = (1.0 / 1.2e-3 + 1.0 / 0.8e-3) / 2.0;
        Config {
            dealias_mode: dealias_mode::STAGGERED_PRT,
            prf_hz: mean_prf_hz as f32,
            prf_ratio_num: 2,
            prf_ratio_den: 3,
            moment_mask: (1 << moment_kind::UZ) | (1 << moment_kind::V),
            ..config_with_thresholds(3.0, 0.4, -100.0)
        }
    }

    /// Serie IQ escalonada `T1,T2,T1,T2,…` perfectamente coherente (sin
    /// ruido, ancho espectral cero): caso límite de la ACF analítica que usa
    /// `crates/staggered-prt/tests/against_oracle.rs`
    /// (`analytic_acf` con `sigma_v=0` colapsa a una fase pura), útil aquí
    /// para probar el cableo con un valor exacto en vez de estadística.
    fn generate_coherent_staggered_channel(
        power_s: f64,
        mean_v: f64,
        wavelength_m: f64,
        t1: f64,
        t2: f64,
        m: usize,
    ) -> Vec<Complex64> {
        let mut times = vec![0.0f64; m];
        for i in 1..m {
            let dt = if (i - 1) % 2 == 0 { t1 } else { t2 };
            times[i] = times[i - 1] + dt;
        }
        times
            .iter()
            .map(|&t| {
                Complex64::from_polar(
                    power_s.sqrt(),
                    4.0 * std::f64::consts::PI * mean_v * t / wavelength_m,
                )
            })
            .collect()
    }

    #[test]
    fn staggered_prt_dealias_recovers_velocity_beyond_single_nyquist() {
        // V_A1≈31.25 m/s (T1=0.8ms), V_A2≈20.83 m/s (T2=1.2ms), V_EXT=62.5
        // m/s (ver `staggered_prt_split`) — mismos valores que
        // `crates/staggered-prt/tests/against_oracle.rs`. v_true=40 m/s cae
        // más allá de las dos Nyquist individuales pero dentro de la
        // extendida: sólo se recupera bien si el desdoblado corre de
        // verdad, no si se publica la medida plegada tal cual.
        const V_TRUE: f64 = 40.0;
        const WAVELENGTH_M: f64 = 0.10;
        const T1: f64 = 0.8e-3;
        const T2: f64 = 1.2e-3;
        const N_GATES: usize = 4;
        const M_PULSES: usize = 32;

        let config = staggered_prt_config();
        let channel: Vec<Vec<Complex64>> = (0..N_GATES)
            .map(|_| {
                generate_coherent_staggered_channel(1.0, V_TRUE, WAVELENGTH_M, T1, T2, M_PULSES)
            })
            .collect();
        let radial = radial_from_channels(vec![channel]);

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { ray, moments } = msg else {
            panic!("se esperaba MomentRay");
        };
        assert_eq!(
            ray.ray_flags & ray_flag::DEALIAS_FAILED,
            0,
            "staggered-PRT nunca marca dealias_failed: el desdoblado es autocontenido en el radial"
        );
        let v_block = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::V)
            .expect("falta el bloque de V");
        for &v in &v_block.values {
            assert!(
                (v as f64 - V_TRUE).abs() < 2.0,
                "V desdoblado ({v}) debería acercarse a v_true={V_TRUE}"
            );
        }
    }

    #[test]
    fn cz_matches_radar_equation_and_uz_stays_uncorrected() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.0 / 1000.0,
            m: 64,
            noise_floor: 0.01,
        };
        let h = generate_cell(&cell, &mut rng);
        let radial = radial_from_channels(vec![vec![h]]);
        let mut config = polarimetric_config((1 << moment_kind::UZ) | (1 << moment_kind::CZ));
        config.start_range_m = 10_000.0; // 10 km: lejos del caso borde rango 0
        config.gate_spacing_m = 250.0;
        config.radar_constant_db = -42.0;

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };
        let uz = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::UZ)
            .expect("falta el bloque de UZ");
        let cz = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::CZ)
            .expect("falta el bloque de CZ");

        assert_eq!(
            cz.field.flags & moment_flag::CORRECTED,
            moment_flag::CORRECTED,
            "CZ debe llevar el flag CORRECTED"
        );
        assert_eq!(
            uz.field.flags & moment_flag::CORRECTED,
            0,
            "UZ es 'sin corregir': no lleva CORRECTED"
        );
        // A 10 km, `20·log10(10) ≈ 20.0 dB` por encima de UZ (más
        // `radar_constant_db`, que UZ nunca aplica).
        let expected = uz.values[0] as f64 + 20.0 * 10.0f64.log10() + config.radar_constant_db as f64;
        assert!(
            (cz.values[0] as f64 - expected).abs() < 1e-3,
            "CZ ({}) debería coincidir con la ecuación del radar aplicada a UZ ({}): esperado {expected}",
            cz.values[0],
            uz.values[0]
        );
    }

    #[test]
    fn cz_is_nan_at_zero_range_even_when_not_censored() {
        let mut rng = StdRng::seed_from_u64(20260901);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 1.0,
            wavelength_m: 0.10,
            prt_s: 1.0 / 1000.0,
            m: 64,
            noise_floor: 0.01,
        };
        let h = generate_cell(&cell, &mut rng);
        let radial = radial_from_channels(vec![vec![h]]);
        // `start_range_m` por defecto de `config_with_thresholds` es 0.0 —
        // valor válido según el contrato, pero sin sentido físico para la
        // ecuación del radar en la celda 0.
        let config = polarimetric_config((1 << moment_kind::UZ) | (1 << moment_kind::CZ));

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { moments, .. } = msg else {
            panic!("se esperaba MomentRay");
        };
        let uz = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::UZ)
            .expect("falta el bloque de UZ");
        let cz = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::CZ)
            .expect("falta el bloque de CZ");

        assert!(
            uz.values[0].is_finite(),
            "UZ no depende del rango: no debería ser NaN"
        );
        assert!(
            cz.values[0].is_nan(),
            "CZ a rango 0 no tiene ecuación del radar válida, aunque la celda no esté censurada"
        );
        assert_eq!(cz.field.flags & moment_flag::HAS_MISSING, moment_flag::HAS_MISSING);
    }

    use lamula_simulator::gaussian_doppler_spectrum;
    use rand_distr::{Distribution, StandardNormal};
    use rustfft::FftPlanner;

    fn complex_gaussian(rng: &mut impl rand::Rng, variance: f64) -> Complex64 {
        let sigma = (variance / 2.0).sqrt();
        let re: f64 = StandardNormal.sample(rng);
        let im: f64 = StandardNormal.sample(rng);
        Complex64::new(re * sigma, im * sigma)
    }

    /// Misma `generate_cell_with_clutter` que
    /// `crates/clutter/tests/against_oracle.rs`: meteoro + clutter casi
    /// puntual en v=0 (`sigma_v=1e-6`), sin promediar — a diferencia de ese
    /// test, aquí se ejercita el cableo con una sola ráfaga, el caso real
    /// del ray builder (ver el doc-comment del módulo, "Sin promediado").
    #[allow(clippy::too_many_arguments)]
    fn generate_cell_with_clutter(
        power_weather: f64,
        mean_v: f64,
        sigma_v: f64,
        power_clutter: f64,
        noise_floor: f64,
        wavelength_m: f64,
        prt_s: f64,
        m: usize,
        rng: &mut impl rand::Rng,
    ) -> Vec<Complex64> {
        let weather = gaussian_doppler_spectrum(power_weather, mean_v, sigma_v, wavelength_m, prt_s, m);
        let clutter = gaussian_doppler_spectrum(power_clutter, 0.0, 1e-6, wavelength_m, prt_s, m);
        let shaped: Vec<Complex64> = weather
            .iter()
            .zip(&clutter)
            .map(|(&w, &c)| complex_gaussian(rng, 1.0) * (w + c).sqrt())
            .collect();
        let mut planner = FftPlanner::new();
        let ifft = planner.plan_fft_inverse(m);
        let mut y = shaped;
        ifft.process(&mut y);
        for x in y.iter_mut() {
            *x += complex_gaussian(rng, noise_floor);
        }
        y
    }

    /// Superpone un tono incoherente con el eco a una ráfaga ya generada —
    /// mismo mecanismo que `generate_cell_full` en
    /// `crates/rfi/tests/against_oracle.rs`, sólo que aquí se aplica sobre
    /// una serie ya en dominio temporal en vez de generarla desde cero.
    fn add_rfi_tone(y: &mut [Complex64], power_rfi: f64, rfi_bin: usize, phase0: f64) {
        let m = y.len();
        for (n, x) in y.iter_mut().enumerate() {
            let phase = 2.0 * std::f64::consts::PI * rfi_bin as f64 * n as f64 / m as f64 + phase0;
            *x += Complex64::from_polar(power_rfi.sqrt(), phase);
        }
    }

    fn clutter_config(clutter_filter_mode: u8, clutter_width_ms: f32, moment_mask: u32) -> Config {
        Config {
            clutter_filter: clutter_filter_mode,
            clutter_width_ms,
            moment_mask,
            ..config_with_thresholds(3.0, 0.4, -100.0)
        }
    }

    const CCOR_MOMENTS_MASK: u32 =
        (1 << moment_kind::UZ) | (1 << moment_kind::CZ) | (1 << moment_kind::CCOR);

    #[test]
    fn clutter_filter_active_marks_flags_and_publishes_ccor() {
        const WAVELENGTH_M: f64 = 0.10;
        const PRT_S: f64 = 1.0e-3;
        const M: usize = 64;

        let v_a = WAVELENGTH_M / (4.0 * PRT_S);
        let bin_spacing = 2.0 * v_a / M as f64;
        let clutter_width_mps = (4.0 * bin_spacing) as f32;

        let mut rng = StdRng::seed_from_u64(20260902);
        let h = generate_cell_with_clutter(
            1.0,   // meteoro
            0.0,   // superpuesto al clutter, el caso que GMAP tiene que resolver
            1.5,
            100.0, // clutter 20 dB más fuerte
            0.01,
            WAVELENGTH_M,
            PRT_S,
            M,
            &mut rng,
        );
        let radial = radial_from_channels(vec![vec![h]]);
        let config = clutter_config(clutter_filter::GMAP, clutter_width_mps, CCOR_MOMENTS_MASK);

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { ray, moments } = msg else {
            panic!("se esperaba MomentRay");
        };

        assert_eq!(
            ray.ray_flags & ray_flag::CLUTTER_FILTERED,
            ray_flag::CLUTTER_FILTERED,
            "filtro activo debería marcar ray_flag::CLUTTER_FILTERED"
        );

        let cz = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::CZ)
            .expect("falta el bloque de CZ");
        assert_eq!(
            cz.field.flags & moment_flag::FILTERED,
            moment_flag::FILTERED,
            "CZ debería llevar moment_flag::FILTERED con el filtro activo"
        );

        let uz = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::UZ)
            .expect("falta el bloque de UZ");
        assert!(
            uz.field.flags & moment_flag::FILTERED == 0,
            "UZ es 'sin filtrar': no debería llevar moment_flag::FILTERED"
        );

        let ccor = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::CCOR)
            .expect("falta el bloque de CCOR");
        assert!(
            ccor.values[0].is_finite() && ccor.values[0] > 0.0,
            "clutter 20 dB más fuerte que el meteoro debería dar CCOR positivo y finito, salió {}",
            ccor.values[0]
        );
    }

    #[test]
    fn clutter_filter_none_omits_ccor_and_filtered_flag() {
        let mut rng = StdRng::seed_from_u64(20260902);
        let cell = CellParams {
            power_s: 1.0,
            mean_v: 3.0,
            sigma_v: 1.5,
            wavelength_m: 0.10,
            prt_s: 1.0e-3,
            m: 64,
            noise_floor: 0.01,
        };
        let h = generate_cell(&cell, &mut rng);
        let radial = radial_from_channels(vec![vec![h]]);
        // moment_mask pide CCOR igual, pero sin filtro activo no hay
        // corrección que reportar -- mismo criterio que los bloques
        // polarimétricos sin segundo canal.
        let config = clutter_config(clutter_filter::NONE, 1.0, CCOR_MOMENTS_MASK);

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { ray, moments } = msg else {
            panic!("se esperaba MomentRay");
        };

        assert_eq!(ray.ray_flags & ray_flag::CLUTTER_FILTERED, 0);
        assert!(
            !moments.iter().any(|m| m.field.kind == moment_kind::CCOR),
            "sin filtro activo no debería publicarse CCOR aunque el moment_mask lo pida"
        );
        let cz = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::CZ)
            .expect("falta el bloque de CZ");
        assert_eq!(cz.field.flags & moment_flag::FILTERED, 0);
    }

    /// `rfi_filter` solo (sin clutter): CZ debe recuperarse de un tono de
    /// RFI fuerte y lejano al pico del meteoro, y no debe publicarse CCOR
    /// ni marcarse `ray_flag::CLUTTER_FILTERED` — el contrato no distingue
    /// "filtrado de RFI" de "filtrado de clutter" con una bandera propia
    /// (ver el doc-comment del módulo).
    #[test]
    fn rfi_filter_alone_recovers_cz_without_publishing_ccor() {
        const WAVELENGTH_M: f64 = 0.10;
        const PRT_S: f64 = 1.0e-3;
        // M=256, como el oráculo de `crates/rfi` -- a M=64 la varianza de un
        // único periodograma (sin promediar, ver el doc-comment del módulo)
        // es demasiado alta para que un tono de RFI se recupere de forma
        // fiable en una sola realización.
        const M: usize = 256;

        let cell = CellParams {
            power_s: 1.0,
            mean_v: 5.0,
            sigma_v: 1.5,
            wavelength_m: WAVELENGTH_M,
            prt_s: PRT_S,
            m: M,
            noise_floor: 0.01,
        };

        let mut rng_clean = StdRng::seed_from_u64(20260902);
        let clean = generate_cell(&cell, &mut rng_clean);
        let mut with_rfi = clean.clone();
        // Bin lejano al pico del meteoro (v≈5 m/s): tono de RFI 20 dB por
        // encima del meteoro, mismo `RFI_BIN=200` que la prueba 1 del
        // oráculo de `crates/rfi`.
        add_rfi_tone(&mut with_rfi, 100.0, 200, 0.7);

        let clean_radial = radial_from_channels(vec![vec![clean]]);
        let rfi_radial = radial_from_channels(vec![vec![with_rfi]]);
        let base = Config {
            start_range_m: 10_000.0, // lejos del caso borde rango 0
            ..clutter_config(clutter_filter::NONE, 1.0, CCOR_MOMENTS_MASK)
        };
        let config_off = Config {
            rfi_filter: 0,
            ..base
        };
        let config_on = Config {
            rfi_filter: 1,
            ..base
        };

        let (msg_clean, _) =
            build_moment_ray(&clean_radial, &config_off, 1, false, 1_000_000, 0.0, None);
        let (msg_unfiltered, _) =
            build_moment_ray(&rfi_radial, &config_off, 1, false, 1_000_000, 0.0, None);
        let (msg_filtered, _) =
            build_moment_ray(&rfi_radial, &config_on, 1, false, 1_000_000, 0.0, None);

        let cz_of = |msg: UpMessage| -> f32 {
            let UpMessage::MomentRay { moments, .. } = msg else {
                panic!("se esperaba MomentRay");
            };
            moments
                .iter()
                .find(|m| m.field.kind == moment_kind::CZ)
                .expect("falta el bloque de CZ")
                .values[0]
        };
        let ray_flags_of = |msg: &UpMessage| -> u8 {
            let UpMessage::MomentRay { ray, .. } = msg else {
                panic!("se esperaba MomentRay");
            };
            ray.ray_flags
        };
        let has_ccor = |msg: &UpMessage| -> bool {
            let UpMessage::MomentRay { moments, .. } = msg else {
                panic!("se esperaba MomentRay");
            };
            moments.iter().any(|m| m.field.kind == moment_kind::CCOR)
        };

        assert_eq!(
            ray_flags_of(&msg_filtered) & ray_flag::CLUTTER_FILTERED,
            0,
            "RFI solo no es 'filtrado de clutter': no debería marcar ray_flag::CLUTTER_FILTERED"
        );
        assert!(
            !has_ccor(&msg_filtered),
            "RFI solo no debería publicar CCOR: no hay corrección de clutter que reportar"
        );

        let cz_clean = cz_of(msg_clean);
        let cz_unfiltered = cz_of(msg_unfiltered);
        let cz_filtered = cz_of(msg_filtered);

        let unfiltered_error = (cz_unfiltered - cz_clean).abs();
        let filtered_error = (cz_filtered - cz_clean).abs();
        assert!(
            unfiltered_error > 3.0,
            "el tono de RFI sin filtrar debería distorsionar CZ de forma medible: limpio={cz_clean} sin_filtrar={cz_unfiltered}"
        );
        // Comparación relativa, no un margen absoluto: un solo periodograma
        // sin promediar (ver "Sin promediado" en el doc-comment del módulo)
        // tiene varianza alta de por sí, así que lo que importa es que
        // filtrar RFI acerque mucho más CZ al valor limpio que dejarlo sin
        // filtrar, no que lo clave con precisión de un estimador promediado.
        assert!(
            filtered_error < 0.3 * unfiltered_error,
            "con rfi_filter activo, CZ debería acercarse mucho más al valor limpio que sin filtrar: limpio={cz_clean} filtrado={cz_filtered} (error {filtered_error:.3}) sin_filtrar={cz_unfiltered} (error {unfiltered_error:.3})"
        );
    }

    /// RFI y clutter activos a la vez: el orden (RFI antes que clutter,
    /// cableado dentro de `clutter_filtered_power`) debe dejar CCOR sano
    /// -- finito y positivo, con clutter 20 dB más fuerte que el meteoro --
    /// pese al tono de RFI superpuesto al hombro de la banda de clutter.
    #[test]
    fn rfi_and_clutter_together_still_publish_sane_ccor() {
        const WAVELENGTH_M: f64 = 0.10;
        const PRT_S: f64 = 1.0e-3;
        // M=256, mismo tamaño que la prueba 3 del oráculo de `crates/rfi`
        // (`filtering_rfi_before_clutter_beats_the_reverse_order`), de donde
        // sale el resto del escenario.
        const M: usize = 256;

        let v_a = WAVELENGTH_M / (4.0 * PRT_S);
        let bin_spacing = 2.0 * v_a / M as f64;
        let clutter_width_mps = (4.0 * bin_spacing) as f32;

        // Bin más cercano a v=4 m/s: dentro del hombro que GMAP usaría para
        // ajustar el modelo gaussiano de la banda de clutter -- mismo
        // cálculo que el oráculo.
        let rfi_bin = (0..M)
            .min_by(|&a, &b| {
                let v_a = bin_velocity(a, M, WAVELENGTH_M, PRT_S);
                let v_b = bin_velocity(b, M, WAVELENGTH_M, PRT_S);
                (v_a - 4.0).abs().partial_cmp(&(v_b - 4.0).abs()).unwrap()
            })
            .unwrap();

        let mut rng = StdRng::seed_from_u64(20260902);
        let h = generate_cell_with_clutter(
            1.0,
            0.0,
            1.5,
            100.0,
            0.05,
            WAVELENGTH_M,
            PRT_S,
            M,
            &mut rng,
        );
        let mut with_rfi = h;
        // Tono de RFI en el hombro de la banda de clutter (mismo escenario
        // que la prueba 3 del oráculo de `crates/rfi`).
        add_rfi_tone(&mut with_rfi, 15.0, rfi_bin, 1.3);

        let radial = radial_from_channels(vec![vec![with_rfi.clone()]]);
        let base = Config {
            start_range_m: 10_000.0, // lejos del caso borde rango 0
            ..clutter_config(clutter_filter::GMAP, clutter_width_mps, CCOR_MOMENTS_MASK)
        };
        let config = Config {
            rfi_filter: 1,
            ..base
        };

        let (msg, _) = build_moment_ray(&radial, &config, 1, false, 1_000_000, 0.0, None);
        let UpMessage::MomentRay { ray, moments } = msg else {
            panic!("se esperaba MomentRay");
        };

        assert_eq!(
            ray.ray_flags & ray_flag::CLUTTER_FILTERED,
            ray_flag::CLUTTER_FILTERED
        );
        let cz = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::CZ)
            .expect("falta el bloque de CZ");
        assert!(
            cz.values[0].is_finite(),
            "CZ no debería quedar censurada con RFI+clutter cableados en el orden correcto"
        );
        // No se exige un signo concreto: con una banda de clutter tan angosta
        // (4 bins de 256) y un único periodograma sin promediar ("Sin
        // promediado", doc-comment del módulo), el resultado exacto del
        // ajuste de GMAP es sensible a en qué bin discreto cae cada frontera
        // -- lo que importa aquí es que la composición RFI-antes-que-clutter
        // corra de punta a punta y publique un CCOR definido, no NaN.
        let ccor = moments
            .iter()
            .find(|m| m.field.kind == moment_kind::CCOR)
            .expect("falta el bloque de CCOR");
        assert!(
            ccor.values[0].is_finite(),
            "CCOR debería salir definido con RFI+clutter cableados en el orden correcto, salió {}",
            ccor.values[0]
        );
    }

    /// Tono puro exacto en un bin de la FFT: `y[n] = amplitud·e^{i2πk n/M}`.
    /// Sin ruido añadido, a diferencia de `generate_cell` — aquí interesa un
    /// valor de verdad-terreno exacto para comparar bit a bit contra una
    /// llamada directa a `spectral_moments`/`pulse_pair_moments` sobre la
    /// misma serie, no la exactitud estadística del estimador (eso ya lo
    /// cubren `crates/spectral/tests/against_oracle.rs` y
    /// `crates/moments/tests/against_oracle.rs`).
    fn pure_tone_series(amplitude: f64, bin_idx: usize, m: usize) -> Vec<Complex64> {
        (0..m)
            .map(|n| {
                let theta = 2.0 * std::f64::consts::PI * bin_idx as f64 * n as f64 / m as f64;
                Complex64::from_polar(amplitude, theta)
            })
            .collect()
    }

    #[test]
    fn spectral_estimator_routes_uz_v_through_periodogram_not_pulse_pair() {
        const M: usize = 64;
        const BIN_IDX: usize = 5;
        const AMPLITUDE: f64 = 2.0;
        const WAVELENGTH_M: f64 = 0.10;
        const PRT_S: f64 = 1.0e-3; // prf_hz = 1000.0, ver config_with_thresholds.

        let y = pure_tone_series(AMPLITUDE, BIN_IDX, M);
        let radial = radial_from_channels(vec![vec![y.clone()]]);

        // Verdad de referencia: llamar directamente a los dos estimadores
        // sobre la MISMA serie que ve el radial. El cableo debe reproducir
        // esto exactamente (salvo redondeo a f32 al pasar por `MomentBlock`);
        // la exactitud de los estimadores en sí no es lo que se prueba aquí.
        let pp_truth = pulse_pair_moments(&y, WAVELENGTH_M, PRT_S);
        let se_truth = spectral_moments(&y, WAVELENGTH_M, PRT_S);
        let se_v_truth = se_truth
            .velocity_mps
            .expect("tono puro muy por encima del umbral de ruido");

        // Umbrales laxos a propósito: esta prueba aísla el cableo del
        // estimador primario, no la censura por SQI/SIG/log_threshold (ver
        // las pruebas de `gate_quality` más arriba para eso).
        let mut config = config_with_thresholds(-100.0, 0.0, -100.0);
        config.moment_mask = (1 << moment_kind::UZ) | (1 << moment_kind::V);
        config.wavelength_m = WAVELENGTH_M as f32;
        config.prf_hz = (1.0 / PRT_S) as f32;

        let moments_for = |estimator_value: u8| -> Vec<MomentBlock> {
            let mut cfg = config;
            cfg.estimator = estimator_value;
            let (msg, _) = build_moment_ray(&radial, &cfg, 1, false, 1_000_000, 0.0, None);
            let UpMessage::MomentRay { moments, .. } = msg else {
                panic!("se esperaba MomentRay");
            };
            moments
        };
        let value_of = |moments: &[MomentBlock], kind: u8| -> f32 {
            moments
                .iter()
                .find(|m| m.field.kind == kind)
                .unwrap_or_else(|| panic!("falta el bloque {kind}"))
                .values[0]
        };

        let pp_moments = moments_for(estimator::PULSE_PAIR);
        let se_moments = moments_for(estimator::SPECTRAL);

        let pp_uz = value_of(&pp_moments, moment_kind::UZ);
        let pp_v = value_of(&pp_moments, moment_kind::V);
        let se_uz = value_of(&se_moments, moment_kind::UZ);
        let se_v = value_of(&se_moments, moment_kind::V);

        assert!(pp_uz.is_finite() && pp_v.is_finite(), "pulse-pair no debería censurar un tono puro");
        assert!(se_uz.is_finite() && se_v.is_finite(), "el modo espectral no debería censurar un tono puro");

        assert!(
            (se_v as f64 - se_v_truth).abs() < 1e-4,
            "V con estimator=spectral ({se_v}) debería coincidir con spectral_moments directo ({se_v_truth})"
        );
        assert!(
            (se_uz as f64 - 10.0 * se_truth.power_linear.log10()).abs() < 1e-4,
            "UZ con estimator=spectral ({se_uz}) debería coincidir con 10·log10(power_linear) de spectral_moments"
        );
        assert!(
            (pp_v as f64 - pp_truth.velocity_mps).abs() < 1e-4,
            "V con estimator=pulse_pair no debería cambiar por cablear el modo espectral"
        );

        // El periodograma con ventana de Hann recorta la línea principal
        // (`docs/algorithms/estimador-espectral.md` §"Cómo funciona"): con
        // fugas espectrales fuera de esa ventana, `power_linear` queda por
        // debajo de la potencia total sin ventanear que usa pulse-pair
        // (`R(0) = amplitud²`, sin ruido que restar). Si los dos modos
        // dieran el mismo UZ, `config.estimator` no estaría cambiando la vía
        // de cómputo.
        assert!(
            se_uz < pp_uz,
            "UZ espectral ({se_uz}) debería quedar por debajo de UZ pulse-pair ({pp_uz}) por el recorte de línea principal; si son iguales, el estimador no se está seleccionando"
        );
    }
}
