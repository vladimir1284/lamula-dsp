# Plan de estudio e implementación

Esta página es el plan de trabajo del conjunto de algoritmos: qué hay que
estudiar, en qué orden, con qué método y con qué criterio se da cada pieza por
terminada. Las páginas individuales describen cada algoritmo; ésta describe el
camino.

## Punto de partida: el contrato promete más de lo que hay documentado

El contrato `DSP↔RCP` v0.1 ya está congelado y expone perillas de configuración
y momentos de salida que no tenían página de algoritmo detrás: `rfi_filter`,
`range_dealias`, `sqi_threshold`, `sig_threshold`, `ccor_threshold`,
`log_threshold`, `clutter_width_ms`, `zdr_offset_db`, `phidp_offset_deg`, el
modo `staggered_prt`, el estimador `spectral` y los momentos ZDR, ΦDP, KDP, LDR,
ρHV, SQI, CCOR y SIG. El trabajo de esta sección es cerrar ese hueco: cada
perilla del contrato tiene que poder rastrearse hasta un algoritmo con
formulación, referencias abiertas y criterio de aceptación numérico.

## Dos ejes de variabilidad del hardware

El DSP no puede asumir una configuración concreta de radar. Todo el conjunto de
algoritmos se diseña sobre dos ejes independientes, y cada página declara
explícitamente qué cambia en cada combinación.

**Eje 1 — fuente de transmisión: magnetrón o klistrón/estado sólido.** Un
magnetrón es un oscilador libre: cada pulso sale con fase inicial aleatoria y
con una frecuencia que deriva con la temperatura y el envejecimiento. Eso obliga
a dos cosas que un transmisor coherente no necesita: medir la fase de cada pulso
en la muestra de burst y restarla de la serie temporal antes de cualquier
estimador Doppler (*coherent-on-receive*), y cerrar un lazo de control automático
de frecuencia (AFC) que reajuste el NCO del receptor. Con klistrón, TWT o
amplificador de estado sólido la fase es determinista y el burst se usa sólo
como referencia de amplitud/fase y monitor de potencia. La consecuencia menos
obvia va en sentido contrario: la fase aleatoria del magnetrón es *explotable*
para separar ecos de segundo trip sin necesidad de codificación programable, que
es justo lo que un transmisor coherente no puede hacer gratis.

**Eje 2 — polarimetría: canal único, simultánea (STAR) o alternante.** Con canal
único sólo existen UZ, CZ, V, W y los índices de calidad. Con transmisión
simultánea H+V y recepción de ambos canales copolares se obtienen ZDR, ΦDP, KDP
y ρHV, pero **no** LDR, y aparece un sesgo por acoplamiento cruzado que hay que
acotar. Con alternancia H/V sí hay LDR, a costa de reducir a la mitad la PRF
efectiva por canal —y con ella la velocidad de Nyquist— y de necesitar
estimadores distintos para ρHV y ΦDP, porque las muestras de los dos canales ya
no son simultáneas.

Consecuencia de diseño, no negociable: **el conjunto de momentos y de modos que
el DSP produce es una capacidad en tiempo de ejecución, no una constante de
compilación.** El contrato ya lo previó con `capability_flags`, `moment_mask`,
`dealias_mask` y `estimator_mask` en el mensaje `capabilities`, y con los
códigos de rechazo `moment_unsupported`, `dealias_unsupported` y
`estimator_unsupported`. El pipeline se construye como una cadena de etapas
opcionales gobernada por esa declaración de capacidades, y el RCP no ofrece al
operador nada que la instalación concreta no sepa hacer.

## Método de estudio: oráculo en Python, luego Rust

Para cada algoritmo se siguen tres pasos, en este orden:

1. **Oráculo en Python** como notebook Jupyter bajo `tools/`, con numpy y
   —cuando exista un análogo— Py-ART, wradlib o LROSE, sobre señal sintética
   de verdad-terreno conocida generada por el
   [simulador de I/Q](simulador-iq.md). El formato notebook es deliberado:
   celdas con gráficas de la señal sintética, del estimador y del error frente
   a la verdad-terreno, todo trazable junto a la fórmula que lo produce.
2. **Implementación en Rust** en el crate correspondiente del pipeline.
3. **Test de contraste numérico** Rust contra el oráculo, con tolerancia
   declarada en la página del algoritmo.

El paso intermedio no es ceremonia. Sin él, «validar contra el simulador» es
validar dos implementaciones propias contra sí mismas: si el error conceptual
está en la interpretación de la formulación, aparece idéntico en el simulador y
en el estimador, y los dos se dan la razón. El oráculo en Python rompe esa
correlación porque se escribe desde el paper, no desde el código Rust, y porque
allí donde existe una implementación abierta madura se contrasta contra ella.

## Criterio de aceptación: varianza teórica, no sólo sesgo

Para los estimadores de momentos, «≤ 2 dBZ y ≤ 1 m/s» es una condición
necesaria pero no suficiente: no dice nada sobre la dispersión del estimador ni
sobre su comportamiento al degradarse la SNR. El criterio de aceptación de cada
estimador se ancla en la varianza teórica publicada en Doviak & Zrnić, capítulo
6, que da la desviación estándar esperada en función del número de muestras M,
de la SNR y del ancho espectral normalizado.

El procedimiento es el mismo para todos: se barre una malla de (SNR, σv, M), se
generan N realizaciones independientes por punto, se comparan sesgo y desviación
estándar medidos contra la curva teórica, y se exige quedar dentro de un margen
declarado. Eso es automatizable y detecta regresiones que una comparación de
valor único deja pasar. Cada página fija su malla y su margen.

## Orden de trabajo

El orden respeta las fases del plan del DSP (§8.2) y las dependencias reales
entre algoritmos.

| Fase | Algoritmos |
| --- | --- |
| 0 (W1–3) | [Simulador de I/Q](simulador-iq.md); kernel numérico (FFT, ventanas, SIMD); arnés de oráculo en Python |
| 1 → M1 (W4–10) | [Ruido y umbrales](ruido-y-umbrales.md); potencia → UZ y [cadena de calibración](reflectivity-calibration.md); [procesamiento de rango y modos de barrido](procesamiento-de-rango.md); [burst, corrección de fase y AFC](burst-fase-afc.md) |
| 2 → M2 (W11–18) | [Pulse-pair](pulse-pair-moments.md); [índices de calidad](indices-de-calidad.md); [estimador espectral](estimador-espectral.md); [GMAP](gmap-clutter-filtering.md) y [mapas de clutter](mapas-de-clutter.md); [filtrado de RFI](rfi-filtrado.md); [dual-PRF](dual-prf-dealiasing.md); [staggered-PRT](staggered-prt.md) |
| 3 → M3 (W19–27) | [Covarianzas polarimétricas](polarimetria-covarianzas.md); [KDP](kdp-estimacion.md); [calibración polarimétrica](calibracion-polarimetrica.md); [dealiasing de rango](dealiasing-de-rango.md); [analizador de espectro de FI](analizador-espectro-fi.md); [corrección de atenuación Z-PHI](atenuacion-zphi.md); [SZ(8/64)](sz-second-trip-recovery.md) (sólo instalación klistrón, promovido desde Stage 2 el 2026-09-04) |
| 4 → M4 (W28–34) | Validación de exactitud contra varianza teórica y regresión Vesta; gates de rendimiento |

**Estado del paso 1 del método (oráculo en Python).** Completo para todo el
trabajo de fase 0 a fase 3, y también, a pedido explícito y pese a que la
propia página lo difiere a Stage 2, para [SZ(8/64)](sz-second-trip-recovery.md)
— el único algoritmo de la tabla que le queda sin oráculo es el kernel
numérico, que no tiene fórmula propia que oracular. Cada uno tiene su notebook
en `tools/oracles/`, enlazado desde la página del algoritmo correspondiente,
ejecutable de punta a punta con `make test-oracles`. El paso 2 (implementación en Rust) y el paso 3 (test de
contraste numérico) están hechos para el simulador de I/Q (`crates/simulator`),
para la mitad con oráculo de [ruido y umbrales](ruido-y-umbrales.md)
(`crates/noise`: estimación HS74, resta, censura por `sig_threshold`) y para
[calibración de reflectividad](reflectivity-calibration.md) (`crates/calibration`:
potencia↔dBZ con constante de radar y corrección por r²) y para
[procesamiento de rango](procesamiento-de-rango.md) (`crates/range`:
asignación de gate, promediado de celda gruesa, composición de split-cut) y
para [burst, fase y AFC](burst-fase-afc.md) (`crates/burst`: fase/frecuencia
del burst, corrección coherent-on-receive, lazo de AFC con congelamiento y
BITE) — completando así toda la fase 1 del plan de trabajo salvo el
ensamblado final de radial. En fase 2, están hechos el paso 2 y el paso 3
para [pulse-pair](pulse-pair-moments.md) (`crates/moments`: potencia,
velocidad y ancho espectral) y para [índices de calidad](indices-de-calidad.md)
(`crates/quality`: SQI, CCOR y SIG) y para
[estimador espectral](estimador-espectral.md) (`crates/spectral`:
periodograma con ventana de Hann, recorte de línea principal y recentrado
circular; el oráculo documenta que su varianza de velocidad no iguala al
pulse-pair en modo unimodal, y que su valor real está en aislar el modo
dominante en escenarios bimodales — cableado en `crates/service::ray` como
selector de `estimator = spectral` para UZ/CZ/V, con fallback a la fase
pulse-pair en celdas censuradas; SQI/SIG siguen calculados sobre la
autocovarianza pulse-pair en este modo, inferencia sin respaldo de oráculo,
ver el doc-comment de `gate_quality`) y para
[GMAP](gmap-clutter-filtering.md) y
[mapas de clutter](mapas-de-clutter.md) (`crates/clutter`: notch, GMAP con
ajuste gaussiano por mínimos cuadrados y degradación explícita a notch, y el
clasificador de persistencia potencia/CV temporal para la generación del
mapa) y para [filtrado de RFI](rfi-filtrado.md) (`crates/rfi`: detección por
exceso sobre la mediana más anchura angosta, interpolación reutilizando sin
reimplementar el `gmap_filter` de `crates/clutter`) y para
[dual-PRF](dual-prf-dealiasing.md) (`crates/dual-prf`: desdoblado por
teorema chino del resto sobre velocidades pulse-pair y corrección por
continuidad espacial) y para [staggered-PRT](staggered-prt.md)
(`crates/staggered-prt`: velocidades pulse-pair sobre las dos subsecuencias
`T1`/`T2` de la misma ráfaga, desdobladas reutilizando sin reimplementar el
mecanismo de `crates/dual-prf`; filtrado de clutter Sachidananda & Zrnić
2000 por descomposición en las dos subsecuencias uniformes y notch por
subsecuencia, alcance de Stage 1 declarado frente a la reconstrucción
gaussiana de `crates/clutter`). Con esto queda completa la fase 2 del plan
de trabajo en los pasos 2 y 3 del método. En fase 3, están hechos el paso 2
y el paso 3 para
[covarianzas polarimétricas](polarimetria-covarianzas.md)
(`crates/polarimetry`: ZDR/ρHV/ΦDP en modo simultáneo; en modo alternante,
ρHV corregido por decorrelación de retardo medio-PRT y ΦDP corregido por el
término de fase Doppler de ese mismo retardo; LDR con saturación por
aislamiento de antena) y para [KDP](kdp-estimacion.md) (`crates/kdp`:
desdoblado de ΦDP y ventana deslizante de mínimos cuadrados, Ryzhkov & Zrnić
1996) y para
[calibración polarimétrica](calibracion-polarimetrica.md)
(`crates/pol-calibration`: offset de ZDR por birdbath y ΦDP de sistema, los
dos por mediana sobre un dwell — la aplicación del offset ya vive en
`crates/polarimetry`, no se repite aquí) y para
[dealiasing de rango](dealiasing-de-rango.md) (`crates/range-dealias`:
detección/marcado dual-PRF y recuperación de primer trip por fase aleatoria
en magnetrón, reutilizando sin reimplementar `crates/burst` y
`crates/moments`) y para
[analizador de espectro de FI](analizador-espectro-fi.md)
(`crates/spectrum-analyzer`: periodograma de Welch con normalización de
ganancia coherente y corrección ENBW explícita para el suelo de ruido) y para
[corrección de atenuación Z-PHI](atenuacion-zphi.md) (`crates/attenuation`:
perfil de atenuación específica cerrado, Testud et al. 2000, cableado sobre
CZ en `crates/service::ray` — el alcance nuevo que "Decisiones cerradas" más
abajo decidió agregarle a CZ) y para
[SZ(8/64)](sz-second-trip-recovery.md) (`crates/sz864`: construcción del
código y separación de dos trips por notch + recoherencia — promovido a
Stage 1 el 2026-09-04 sólo para instalación klistrón, ver "Decisiones
cerradas"; sin cablear en `crates/service::ray`, ver la página del
algoritmo). Con esto queda completa la fase 3 del plan de trabajo en los
pasos 2 y 3 del método para toda instalación klistrón; con magnetrón, SZ(8/64)
no aplica y la fase 3 ya estaba completa sin él.

**Dependencia que mueve una pieza de fase.** El plan sitúa el burst/AFC en la
fase 2, junto al resto de la suite Doppler. Si la instalación es de magnetrón,
la corrección de fase es prerrequisito duro del pulse-pair —sin ella la serie
temporal no es coherente y el estimador de velocidad no significa nada— y sube a
la fase 1. Por eso aparece en la fase 1 de la tabla: es el caso que cubre las
dos configuraciones. Con transmisor coherente la etapa se reduce a monitor de
potencia y puede quedarse donde el plan la puso.

## Decisiones cerradas

Tres puntos que no se resolvían documentando, sino decidiendo. Quedaron
abiertos hasta 2026-09-02; esta sección registra la resolución y el trabajo
que cada una desbloquea, para que no se pierda ninguna de las dos cosas.

**`range_dealias` sin SZ — cerrada: un bit, semántica por hardware.** El
contrato ofrece recuperación de trip múltiple en v0.1 y la página de
[SZ(8/64)](sz-second-trip-recovery.md) la difiere a Stage 2. Se ratifica la
postura que ya proponía esta página: Stage 1 declara el bit `range_dealias`
(`capability_flag`, valor 16) según el hardware de la instalación —
magnetrón: recuperación real por fase aleatoria; transmisor coherente sin
codificación programable: sólo detección y marcado (censura, no corrige). El
RCP no distingue cuál de las dos hace el DSP detrás del mismo bit. No
requiere cambio de contrato. El nivel "detección y marcado" ya está cableado
en `crates/service::ray` (`config.range_dealias`, cross-radial vía
`PreviousPrf`, independiente de `dealias_mode`) con un criterio de
emparejamiento celda-a-celda que es **inferencia mía sin respaldo de
oráculo** — ni `classify_trip` de `lamula-range-dealias` ni su oráculo
cubren una malla de eco distribuido, sólo un blanco puntual; el doc-comment
junto a `range_dealias_detected` en `build_moment_ray` explica el criterio
usado y por qué. La recuperación por fase aleatoria en magnetrón sigue sin
cablear (ver [dealiasing de rango](dealiasing-de-rango.md) y el doc-comment
de `crate::main`): falta fase de burst por pulso en el wire, y falta un
campo de hardware en el contrato con que decidir si aplicaría.

**Modo de polarización de la instalación — cerrada: campo nuevo en el
contrato. Campo agregado, cableo a `crates/service::ray` sigue pendiente.**
El enum `moment_kind` promete LDR, y LDR sólo existe en modo alternante o con
un canal cruzado dedicado. `crates/polarimetry` ya implementa
`polarimetric_moments_alternating` y `ldr_db`, contrastados contra oráculo —
el hueco no era de algoritmo, era de contrato: sólo existía `n_rx_channels`
(conteo), sin campo que dijera si ese segundo canal es simultáneo (STAR) o
alternante H/V. Se decidió agregar un campo `polarization_mode`
(simultáneo=0/alternante=1) al contrato `DSP↔RCP`, como patch de v0.1 a v0.2
(`version_minor` 1→2): consume el `pad0: u32` que ya reservaba `config` sin
crecer el mensaje (`polarization_mode: u8` + `pad0: u8` + `pad1: u16`, mismos
4 bytes) — esto reemplaza el marco anterior de la decisión ("le corresponde
al equipo DRx fijar el `channel_mask`"): la responsabilidad de DRx es el
cableado físico de canales, la de este contrato es declarar el modo. Hecho:
esquema, regeneración de `contract/generated/` (Rust/Python/TS) y los sitios
manuales que codificaban `Config` byte a byte (`crates/rcp-link/src/wire.rs`
y los tests de `crates/rcp-link`/`crates/service`). **Sin verificar**:
`cargo build`/`cargo test` del workspace — este entorno no tiene `cargo`
disponible; sólo se corrió la batería de contraste de codegen en Python
(`contract/tests`, 71/71).

**Lo que este campo NO resuelve todavía, y por qué es tarea aparte**: sólo
declara el modo, no cablea `polarimetric_moments_alternating`/`ldr_db` en
`crates/service::ray`. Hacerlo de verdad tropieza con huecos propios, no con
falta del campo: (1) `AssembledRadial.channels[c][bin]` no documenta en
ningún sitio la convención canal↔polarización en modo alternante — si
`channels[0]` es "copolar" fijo por hardware (HH en pulsos de transmisión H,
VV en pulsos de transmisión V) o si el mapeo es al revés, y qué paridad de
pulso corresponde a cuál transmisión; ningún oráculo de
`crates/ingest`/`tools/oracles/polarimetria_covarianzas.ipynb` cubre el
ensamblado de radial, sólo el estimador ya con `h`/`v` separados. (2) El
pulse-pair de UZ/V/SQI/SIG hoy corre sin condicionar sobre
`radial.channels[0]` entero asumiendo una serie coherente a un único PRT —
en modo alternante esa serie mezcla ecos de transmisión H y V pulso a pulso,
y la autocovarianza a retardo 1 dejaría de medir sólo fase Doppler (se
contaminaría con ZDR): hace falta un pulse-pair propio de modo alternante
sobre las dos subsecuencias de igual polarización, mismo tipo de trabajo que
`staggered_pulse_pair_velocities` le hizo falta a staggered-PRT, no reutilizar
el de canal único sin más. (3) ~~`ldr_db` pide `antenna_isolation_db`, que no
tiene campo en `Config`~~ — **cerrado** (ver más abajo). (4)
`polarimetric_moments_alternating` pide `sigma_v_mps` (ancho espectral) ya
estimado por celda; con el pulse-pair de canal único eso existe
(`PulsePairEstimate::spectrum_width_mps`), pero con el pulse-pair propio de
(2) todavía sin escribir no hay de dónde tomarlo.

**(1) era más grave de lo que esta sección decía: no era sólo falta de
documentación, era falta de campo en el contrato upstream — parcialmente
cerrado.** El contrato `DRx↔DSP` v0.1 vendorizado no tenía ningún campo por
pulso que indicara si ese pulso transmitió H o V: `ray_flags` sólo definía
`FIRST_AFTER_CONFIG` (bit 8); `pulse_mode` es el modo de ancho de pulso
vigente y `channel_mask` son los canales físicos de recepción presentes,
ninguno de los dos codifica polarización de transmisión.
`polarimetric_moments_alternating` (`crates/polarimetry/src/covariance.rs`)
exige `h[]`/`v[]` ya separados por paridad de pulso a retardo medio-PRT; sin
ese bit en el cable, `crates/ingest::assembly::RadialAssembler` no tenía con
qué separar la serie intercalada en dos subseries por polarización.

Se pidió el campo al proyecto `lamula-drx` y ya existe: bit `tx_pol_v`
(valor 16) en el enum `ray_flag`, `DRx↔DSP` v0.1 → v0.2 (aditivo, uno de los
4 bits libres que quedaban tras `azel_invalid`/`ddc_overflow`/`truncated`/
`first_after_config` — no crece el mensaje `Ray`, así que fue
`version_minor`, no ruptura). Commit `31cec50` en `lamula-drx`, pusheado a
`origin/main`. Revendorizado acá: `contract/vendor/UPSTREAM.toml` apunta a
ese commit, `contract/vendor/drx_dsp_v0_1.{rs,py}` regenerados y verificados
contra su hash (`tools/check_vendored_contract.py --strict` en verde).

**(a)+(b) hechos.** `RawPulseFrame` ya traía `ray_flags` decodificado del
cable (`crates/ingest/src/wire.rs`); lo que faltaba era que
`RadialAssembler::finish` lo descartaba al armar el radial. Ahora
`AssembledRadial` lleva `ray_flags: Vec<u8>`, un byte por pulso en el mismo
orden que `channels[c][bin]`, y expone
`AssembledRadial::split_by_tx_polarization(series)`, que separa cualquier
serie de pulsos de un canal en subseries H/V por paridad del bit
`ray_flag::TX_POL_V` preservando el orden de llegada. Con esto la convención
canal↔polarización que (1) pedía documentar deja de ser implícita: se lee
del wire, no se asume. En canal único o simultánea (STAR) el bit nunca se
fija y la subserie V sale vacía — no-op confirmado por test. Cambio acotado
a `crates/ingest::assembly` (`AssembledRadial`, `RadialAssembler::finish`) más
el ajuste del helper de test de `crates/service::ray` que construye
`AssembledRadial` a mano; contraste numérico no aplica aquí (no hay fórmula,
es reordenamiento de datos), cubierto con tests unitarios de
`crates/ingest`. `cargo build`/`cargo test --workspace` en verde.

**(2) y (4) — cerrados: `crates/service::ray` ya consume la subserie H/V.**
`split_by_tx_polarization` ya tiene llamador. UZ/V/SQI/SIG (y, cuando están
activos, el estimador espectral alternativo y el filtro de clutter/RFI) ya
no corren sobre `radial.channels[0]` entero en modo alternante: corren sobre
`main_channel`, la subserie copolar H (paridad `TX_POL_V` = 0), con su PRT
propio doblado (`own_prt_for_main` — se salta un pulso de cada dos, PRF
efectiva a la mitad, tal como describe `docs/algorithms/
polarimetria-covarianzas.md` §"Configuraciones cubiertas"). No hizo falta un
estimador nuevo: a diferencia de staggered-PRT (dos retardos distintos, T1 y
T2), la subserie copolar tiene espaciado UNIFORME (2×PRT nominal), así que
`lamula_moments::pulse_pair_moments` se reutiliza tal cual con el PRT
correcto — el "propio" que (2) pedía es la partición y el escalado del PRT,
no una fórmula distinta. `polarimetric_moments_alternating` ya se llama con
`hh` (= `main_channel`, reutilizado, no partido dos veces) y `vv` (partiendo
`channels[1]`); `sigma_v_mps` sale de `spectrum_width_mps` del propio
pulse-pair principal sobre esa misma celda — el origen que (4) dejaba
pendiente. `ldr_db` también cableado: `vh` sale de partir `channels[1]` por
la misma paridad, `antenna_isolation_db` ya venía del contrato (cerrado más
abajo). LDR se censura a NaN si la celda ya está censurada por SNR o si
`LdrEstimate::reliable` es falso — publicarlo sin fiabilidad es "engañoso",
como dice la página del algoritmo. `nyquist_velocity` en la rama sin
dealiasing por radial usa `own_prt_for_main`, así que la Nyquist ya sale
reducida a la mitad en alternante sin rama aparte. Test de cableo (no de
exactitud, ese ya está en `crates/polarimetry/tests/against_oracle.rs`) en
`ray::tests::alternating_polarization_wiring_uses_split_subsequences_not_naive_simultaneous`:
compara el ZDR/LDR obtenidos por el split correcto contra el cómputo
ingenuo (simultáneo sobre la serie intercalada) y confirma que difieren —
el error clásico que la página del algoritmo señala. `cargo build
--workspace`/`cargo test --workspace` limpios.

**Lo que esto NO resuelve todavía**: la combinación polarización alternante
+ `dealias_mode` `DUAL_PRF`/`STAGGERED_PRT` en el mismo radial no está
contrastada contra ningún oráculo — ninguna de esas dos ramas de
`nyquist_velocity`/`dual_prf_split`/`staggered_prt_split` usa
`own_prt_for_main`, así que esa combinación (poco realista: alternar
polarización Y PRF/PRT a la vez) queda sin comportamiento verificado, sólo
sin pánico.

**Alternante + `estimator = spectral` / filtro de clutter/RFI — cerrado, sin
fórmula nueva.** Las dos ramas ya se enrutaban por el mismo
`main_channel`/`own_prt_for_main` que el pulse-pair principal, por
consistencia (evitar la misma contaminación de (2) en cualquier otro
consumidor de "el canal 0 tal cual"), pero eso no estaba probado — sólo
inferido por analogía. Al revisarlo no hacía falta oráculo nuevo:
`spectral_moments`/`clutter_filtered_power` no le piden nada especial a su
serie de entrada más que ser uniforme y coherente a un PRT conocido, que es
exactamente lo que `main_channel` ya garantiza (mismo argumento que (2) usó
para justificar reutilizar `pulse_pair_moments` sin más). Dos tests de cableo
nuevos en `ray::tests`
(`alternating_polarization_wiring_routes_spectral_estimator_through_main_channel`,
`...routes_clutter_filter_through_main_channel`): cada uno arma un radial
alternante donde los pulsos de transmisión V llevan una señal deliberadamente
muy distinta (un tono aislado mucho más fuerte) a la de la subserie H, y
confirma que V/CCOR coinciden con `spectral_moments`/`clutter_filtered_power`
llamados directamente sobre la subserie H sola, y difieren claramente de
correr esos mismos estimadores sobre `channels[0]` intercalado completo al
PRT crudo — mismo criterio que la prueba de ZDR/LDR de arriba. Nota de
depuración: la primera versión del test de clutter falló por 0.4 dB con la
verdad de referencia calculada a partir de un literal `f64` para
`wavelength_m` en vez de `config.wavelength_m as f64` (el `f32` real que usa
el cableo) — con el ancho de la ventana de clutter elegido como múltiplo
exacto del espaciado de bin, el redondeo de punta a punta bastaba para mover
un bin de borde al otro lado del umbral; no era un bug de `crates/service`,
era la verdad de referencia del test tomando un camino de redondeo distinto
al del cableo real. `cargo test -p lamula-dsp-service`/`cargo clippy -- -D
warnings`/`cargo fmt --check` en verde.

**ΦDP en modo alternante, término de fase Doppler del retardo medio-PRT —
cerrado, fórmula.** El hueco que el propio oráculo de `crates/polarimetry`
señalaba desde antes del cableo de (1)-(4) arriba: `arg(R_hv)` a retardo
medio-PRT `T` no es sólo `ΦDP_verdadero`, acarrea además la fase de Doppler de
la autocorrelación temporal del blanco a ese retardo —misma forma que la fase
de `R(1)` en pulse-pair, `θ_doppler = -4π·v·T/λ`, evaluada a `T` (medio PRT)
en vez del PRT completo—. Derivado del modelo de covarianza temporal ya
validado en la celda "Modo alternante" del oráculo (`temporal_covariance`
tenía la fase desde el principio; lo que faltaba era una celda que
contrastara ΦDP, no sólo ρHV, contra ella): con `h[i]` la muestra anterior en
el tiempo y `v[i]` la posterior (mismo orden que `generate_alternating_dualpol`
genera y que `crates/service::ray` ya cablea), `arg(E[h·conj(v)]) =
ΦDP_verdadero + θ_doppler`, así que `ΦDP_verdadero = arg(R_hv) − θ_doppler −
phidp_offset_deg`. `polarimetric_moments_alternating`
(`crates/polarimetry/src/covariance.rs`) gana un parámetro nuevo,
`velocity_mps` — la velocidad radial media ya estimada para la celda, mismo
origen y mismo criterio de "no re-estimar aquí" que `sigma_v_mps`
(pulse-pair sobre el canal H a su propio PRT) —, y resta `θ_doppler` de
`arg(R_hv)` antes de envolver a `(-180, 180]`. Cableado en
`crates/service::ray`: la llamada ya tenía `sigma_v_mps =
estimates[i].spectrum_width_mps`, ahora también pasa `estimates[i]
.velocity_mps` — el mismo `PulsePairEstimate` de esa celda, ningún cómputo
nuevo. Contraste numérico nuevo en
`crates/polarimetry/tests/against_oracle.rs`
(`alternating_phidp_corrected_matches_truth_naive_is_biased`): compara el
sesgo de ΦDP corregido (`velocity_mps=MEAN_V`, dentro de `BIAS_TOL_PHIDP`)
contra el mismo estimador sin corregir (`velocity_mps=0.0`, sesgo de
~36° a los parámetros del oráculo, muy por encima de la tolerancia) —
confirma que el efecto es real, no un artefacto de muestra finita, mismo
criterio que la comparación análoga de ρHV. Oráculo actualizado con las
celdas correspondientes antes del cambio Rust, como pide el método —
ejecutado (`jupyter nbconvert --execute`, venv del repo) con sus 13
comprobaciones en verde, las dos nuevas de ΦDP incluidas. `cargo build`/
`cargo test --workspace`/`cargo clippy -- -D warnings`/`cargo fmt --check`
en verde.

**Nota aparte, no cerrada por este cambio**: el enum `polarization_mode` del
esquema (`contract/schema/dsp_rcp_v0_1.toml`) describe `alternating` como
"H/V alternante **radial a radial**", pero `ray_flag::TX_POL_V` es un bit
POR PULSO y todo este cableo (y el propio pedido a `lamula-drx` que lo
originó, ver arriba) asume alternancia pulso a pulso — la misma que describe
`docs/algorithms/polarimetria-covarianzas.md` ("H/V conmutados pulso a
pulso"). Ese texto del esquema quedó desactualizado de una redacción anterior
al bit `TX_POL_V`; no se corrige aquí porque tocar el esquema exige el mismo
trámite de versión que `polarization_mode`/`antenna_isolation_db` (ver
arriba) para un cambio que es sólo de documentación, no de formato — queda
señalado para la próxima revisión del contrato.

**`antenna_isolation_db` — cerrado: campo nuevo en `Config`, `DSP↔RCP` v0.2 →
v1.0.** A diferencia de `polarization_mode`, no quedaba relleno suficiente en
`Config` para el campo (sólo 3 B de `pad0`+`pad1`, un `f32` necesita 4): se
agregó como campo nuevo entre `phidp_offset_deg` y `wavelength_m`, creciendo
`Config` de 80 a 84 B. Por la propia regla del contrato ("cualquier cambio de
formato sube `version_minor` (compatible) o `version_major` (rompe)") y
porque crecer el mensaje es justo lo que se evitó deliberadamente al agregar
`polarization_mode`, esto se trató como ruptura: `version_major` 0→1,
`version_minor` reiniciado a 0. Regenerados `contract/generated/` (Rust/
Python/TS) y corregidos los sitios manuales que codificaban `Config` byte a
byte (`crates/rcp-link/src/wire.rs`, `crates/contract/tests/layout.rs`, y los
tests de `crates/rcp-link`/`crates/service`). Verificado: `cargo build
--workspace` y `cargo test --workspace` limpios, y `contract/tests` (71/71)
— a diferencia del cambio anterior, esta vez sí había `cargo` disponible en
el entorno. El campo sólo se declara; `ldr_db` sigue sin cablearse en
`crates/service::ray` porque eso depende de (1), no de este campo.

**SQI/CCOR/SIG bajo `estimator = spectral` — cerrada: quedan atados a la
autocovarianza pulse-pair siempre, no a `SpectralEstimate`, sin fórmula
espectral nueva.** `crates/service::ray::gate_quality` marcaba esto como
"inferencia mía sin respaldo de oráculo" desde el cableo del estimador
espectral (`b8bcda9`). Revisado sin acceso a `cargo`/`jupyter` en este
entorno — lo que descarta derivar y validar una fórmula espectral nueva de
la misma forma que fase 4 ya renunció a improvisar la varianza teórica de
pulse-pair sin poder contrastarla — se concluye que no hace falta fórmula
nueva: los tres índices caracterizan la serie cruda/filtrada (coherencia a
retardo 1, SNR, razón de filtrado), no el algoritmo que luego decide
velocidad/potencia publicadas. Definir un "SQI espectral" en términos de la
potencia del lóbulo principal recortado filtraría un parámetro interno del
estimador (`DROP_DB`/semiancho máximo de `crates/spectral`) dentro de un
índice de calidad del contrato — el mismo eco cambiaría de SQI sólo por
cambiar `config.estimator`, rompiendo la invariancia que
`sqi_threshold`/`sig_threshold` necesitan para significar lo mismo entre
modos. Ver [índices de calidad](indices-de-calidad.md) §"Configuraciones
cubiertas". Cambio de sólo documentación/comentarios, ningún cambio de
lógica en `crates/service::ray`.

**Qué significa exactamente CZ — cerrada: se expande a incluir corrección de
atenuación. Implementada.** Por herencia de Vesta/Sigmet, CZ era hasta ahora
"reflectividad tras el filtro de clutter" (`crates/service::ray`); UZ sigue
siendo la reflectividad sin filtrar. Se decidió que CZ incluya además
corrección de atenuación vía Z-PHI (Testud et al. 2000) — alcance nuevo, no
una aclaración de documentación. Los tres pasos del método están hechos:
página del algoritmo ([corrección de atenuación Z-PHI](atenuacion-zphi.md)),
oráculo (`tools/oracles/atenuacion_zphi.ipynb`) y crate nuevo
(`crates/attenuation`, no una extensión de `crates/calibration`/
`crates/polarimetry` — la fórmula de Testud es lo bastante propia como para
justificar su propio crate), cableado sobre CZ en `crates/service::ray` para
cada tramo contiguo con segundo canal válido, con test de contraste
(`crates/attenuation/tests/against_oracle.rs`) y test de cableo propio
(`ray::tests::zphi_correction_recovers_attenuated_cz_on_dual_channel_radial`).
El criterio de aceptación es, como se anticipaba, la restricción de
autoconsistencia (no sólo sesgo) — ver "Cómo funciona" de la página del
algoritmo. **Fórmula reconstruida de memoria de la literatura, verificada
contra Py-ART pero no contra el paper original de Testud** (sin acceso a él
en este entorno) — la página del algoritmo lo señala como pendiente de
contrastar antes de tratar el crate como validado externamente. Los
coeficientes `β`/`a_coef` de la fórmula quedan fijos como constantes locales
en `crates/service::ray` (banda C por defecto), mismo tipo de hueco sin
campo propio en el contrato que `polarization_mode` — ver arriba.

**Burst/AFC (`crates/burst`) → `crates/service::ray` — cerrado el hueco de
contrato, cableada la corrección de fase; el lazo de AFC ya cablea también (ver
más abajo).**
`crates/burst` (medida de fase/frecuencia del burst, corrección
coherent-on-receive, lazo de AFC) estaba implementado y contrastado contra
oráculo desde que se agregó el crate, pero no se podía cablear: `channel_mask`
no tenía tabla de bits (a diferencia de `ray_flag`/`bite_flag`), así que nada
decía qué canal, de los que trae `channels[c][bin]`, es un canal de burst —
mismo tipo de hueco de contrato ya cerrado para `ray_flag::tx_pol_v`.
Verificado contra el proyecto DRx (`rtl/ssa/drx_ssa_pkg.sv`: `N_RX_CH=4`,
`N_TX_BURST_CH=2`) que el hardware sí reserva los dos canales de burst, pero
ningún artefacto (contrato, firmware, RTL) asignaba bit/índice concreto a
cada uno.

Se pidió el campo al proyecto DRx: enum `channel` sobre `channel_mask`
(`rx_0..rx_3` bits 1/2/4/8, `tx_burst_0`/`tx_burst_1` bits 16/32), `DRx↔DSP`
v0.2 → v0.3, aditivo (commit `61edc824882dfc7b1cc8919d41f25bffbf6509d2` en
`lamula-drx`, revendorizado acá). El orden de `channels[]` (ascendente por
bit puesto) y que un canal de burst trae los mismos `bins` que el resto del
rayo (energía sólo durante la ventana del pulso, silencio el resto) quedaron
documentados en el propio enum — inferencia razonable, no confirmada contra
RTL/firmware más allá del conteo de canales, señalada como tal en la
solicitud.

Segundo hueco encontrado al cablear: ni siquiera con el canal identificado
había forma de saber CUÁNTOS bins iniciales de ese canal son ventana de
burst real — depende de `pulse_width_idx`/decimación, específico de la
instalación. Se decidió resolverlo del lado `DSP↔RCP` en vez de pedir otro
campo a DRx (es config de instalación, mismo tipo que `zdr_offset_db`/
`antenna_isolation_db`): campo nuevo `burst_window_bins` (`u16`) en `Config`,
consumiendo el `pad1` que quedaba libre tras `polarization_mode` — aditivo,
`CONFIG_SIZE` se mantiene en 84 B, `version_minor` v1.0 → v1.1. `0` significa
"sin canal de burst" (transmisor coherente sin monitor, o instalación sin
ese cableado).

Con los dos campos ya declarados, `crates/service::ray::burst_phase_correct`
mide la fase de `channel::TX_BURST_0` por pulso
(`lamula_burst::burst_phase_estimate` sobre `AssembledRadial::burst_window`,
nuevo helper mecánico en `crates/ingest`, mismo tipo de trabajo que
`split_by_tx_polarization` — sin fórmula propia, cubierto con test unitario
en vez de contraste de oráculo) y corrige con
`lamula_burst::correct_phase` todo canal que no sea el propio canal de burst,
antes de que corra cualquier otra cosa (pulse-pair, polarimetría, clutter,
RFI, dealiasing) — cableado por un `radial` sombreado al principio de
`build_moment_ray`, sin tocar la lógica aguas abajo. Sólo consume
`TX_BURST_0`; `TX_BURST_1`, si está presente, queda sin usar en este cableo
inicial (redundancia de hardware, no un algoritmo distinto). Test de cableo
(`ray::tests::burst_phase_correction_recovers_velocity_from_magnetron_pulse_to_pulse_phase_noise`):
radial sintético con fase aleatoria pulso a pulso inyectada por igual en eco
y burst (como saldría del mismo pulso transmitido) — sin corregir, V no se
acerca a v_true; corregido, sí. `cargo test --workspace` limpio.

**Lazo de AFC — cerrado, cableado de punta a punta; conversión Hz→palabra de
NCO sin verificar contra hardware real.** El bloqueo que dejaba esto sin
cablear era de transporte, no de algoritmo: `crates/ingest` sólo tenía camino
de lectura sobre la conexión DRx. Se agregó camino de escritura a
`crates/ingest::tcp` (`IngestSource::afc`, un canal `mpsc` drenado por una
tarea aparte que escribe sobre la mitad de escritura vigente del socket
aceptado — `TcpStream::into_split`, coordinada con la tarea de lectura vía
`Arc<Mutex<Option<OwnedWriteHalf>>>`, ver el doc-comment del módulo) y
`crates/ingest::wire::encode_afc_frame` (serialización del mensaje `Afc`,
simétrica de `lamula_simulator::pack_rays` en sentido inverso); `simulator` y
`udp` aceptan y descartan `afc` (no tienen DRx real del otro lado). Los
adapters `simulator`/`udp` no cambian de comportamiento, sólo de forma —
`cargo test --workspace` limpio.

Con transporte resuelto, `crates/burst` gana
`nco_phase_inc_for_freq_offset(freq_offset_hz, fs_hz, word_bits)`: la
convención DDS estándar de acumulador de fase que espera `nco_phase_inc`
(decisión D-02 del proyecto DRx, ver la página del algoritmo), parametrizada
en `fs_hz`/`word_bits` en vez de fijarlos — **ninguno de los dos viene de
ningún contrato de este repositorio**: ni `DRx↔DSP` ni `DSP↔RCP` exponen la
frecuencia de referencia ni la anchura del acumulador de fase del NCO de
recepción del DRx (`docs/dsp-plan.md` sólo documenta 250 MSPS como reloj del
ADC, sin confirmar si el NCO deriva de ese mismo reloj). Se resolvió siguiendo
el patrón ya establecido para huecos de este tipo (`ServiceConfig`, igual que
`full_scale_counts`/la resolución del encoder SSI): dos variables de entorno
nuevas y obligatorias, `LAMULA_DSP_DRX_NCO_FS_HZ`/`LAMULA_DSP_DRX_NCO_WORD_BITS`,
sin valor por defecto inventado — bloquea confirmar la corrección real contra
hardware DRx, no bloquea el resto del cableo. `crates/service::ray` gana
`afc_burst_sample` (pulso 0 del canal de burst, gated por
`MAGNETRON_TRANSMITTER` igual que `recover_trip1`) y `fast_time_dt_s` — este
último SÍ con respaldo de contrato: se deriva de `config.gate_spacing_m`
(`2·gate_spacing_m/c`), no de ninguna constante de reloj inventada.
`crates/service::main` crea/recrea `AfcLoop` en `START` (ganancia vía
`lamula_burst::loop_gain(n_pulses/prf_hz, afc_tau_s)`, con `afc_tau_s`/
`afc_amp_threshold` como otro par de variables de entorno obligatorias del
mismo tipo) y lo alimenta una vez por radial, mandando el `Afc` resultante por
`ingest.afc`. El congelamiento/BITE de `AfcUpdate::bite` ante pérdida de burst
no se publica en ningún sitio todavía — no hay Status & BITE Manager en este
workspace, mismo alcance que ya declaraba `crate::main` para el resto de BITE.
Tests: contraste puro de la conversión Hz→palabra en `crates/burst`
(cuarto de vuelta, envuelto negativo, envuelto a escala completa), cableo del
socket en `crates/ingest/tests/afc_write_path.rs` (llega tal cual antes de
cualquier conexión se descarta en silencio), cableo de la extracción de
muestra en `ray::tests::afc_burst_sample_*`. `cargo test --workspace`/
`cargo clippy --all-targets -- -D warnings`/`cargo fmt --check` en verde.

**`recover_trip1` de [dealiasing de rango](dealiasing-de-rango.md) — cerrado,
sin llamar literalmente a la función del crate.** El bloqueo de contrato que
lo impedía (fase de burst por pulso en el wire) ya no existe desde el cableo
de burst/AFC de arriba. Al revisar cómo cablearlo, `crates/service::ray`
(`build_moment_ray`) resultó que ya no hacía falta invocar
`lamula_range_dealias::recover_trip1`: `burst_phase_correct` corrige TODA
celda de todo canal con la fase de burst de su propio pulso antes de
cualquier pulse-pair, que es exactamente lo que hace `recover_trip1` celda a
celda (corregir con la fase del primer trip, dejar el segundo con fase
residual uniforme que HS74 blanquea a ruido). Las celdas con evidencia de
trip2 en una instalación de magnetrón con burst wireado ya llegan al bloque
de detección con el valor recuperado — sólo faltaba dejar de sobreescribirlo
con `NaN`. Se agregó la constante local `MAGNETRON_TRANSMITTER` (mismo tipo
de hueco sin campo propio en el contrato que `ZPHI_A_COEF_DB_PER_DEG`, ver
más arriba) para no aplicar esta misma lógica en transmisor coherente, donde
el segundo trip es igual de determinista que el primero y corregir con la
fase de éste no decorrelaciona nada — ahí sigue aplicando sólo detección y
marcado. Test de cableo nuevo
(`ray::tests::range_dealias_recovers_trip2_evidenced_cell_when_burst_is_wired`),
hermano del ya existente de censura pura; `cargo build`/`cargo test
--workspace` limpios.

**Bug encontrado y corregido en el mismo cableo**: escribiendo el test de
arriba, un radial con canal único (RX_0) más canal de burst (`TX_BURST_0`)
—exactamente el caso de instalación magnetrón mono-polar con monitor de
burst que describe el Eje 1 de esta página— hacía entrar en pánico a
`crates/service::ray`. La causa: el cableo polarimétrico
(`radial.channels.len() > 1`, en dos sitios de `build_moment_ray`) decidía
si había canal cruzado por POSICIÓN, no por qué bit de `channel_mask` es,
así que interpretaba el canal de burst como si fuera el canal V — con menos
bins que el canal principal (el caso real: `burst_window_bins` casi siempre
mucho más chico que `n_bins`), esto reventaba por índice fuera de rango; con
igual número de bins no reventaba, pero calculaba ZDR/ρHV/ΦDP/LDR sobre
datos de burst sin sentido (no se publicaban porque `moment_mask` no los
pide, pero se calculaban igual). No era un bug de este cableo — existía
desde antes, éste sólo lo hizo visible. **Corregido**: los dos sitios ahora
usan `radial.channel_index(channel::RX_1)` (`v_channel_idx`) en vez de
`channels.len() > 1`/indexado posicional `channels[1]` — sólo hay canal
polarimétrico si el bit `RX_1` está puesto en `channel_mask`, sea cual sea
la posición o qué otros canales traiga el radial. Test de regresión
`ray::tests::burst_channel_alone_does_not_trigger_polarimetric_wiring`
(radial `RX_0 | TX_BURST_0` con bins desiguales, antes reventaba, ahora no
publica ZDR aunque se pida). `cargo build`/`cargo test --workspace`
limpios, `cargo fmt` aplicado.

**Analizador de espectro de FI (`crates/spectrum-analyzer`) →
`crates/service::ray` — cerrado el hueco de contrato, cableada la captura
oportunista.** `crates/spectrum-analyzer` (periodograma de Welch, ganancia
coherente, ENBW) estaba implementado y contrastado contra oráculo, y el
mensaje `spectrum_frame` ya existía en el contrato desde v1.0, pero no había
forma de que el RCP lo pidiera: la enumeración `command` no tenía ningún
mandato para `spectrum_frame`, a diferencia de `request_status`/
`request_capabilities` para el resto de mensajes `up` bajo demanda.

Se agregó `request_spectrum` (valor 7) a la enumeración `command`, aditivo,
`DSP↔RCP` v1.1 → v1.2 — mismo criterio de versión que `burst_window_bins` más
arriba: no cambia el tamaño de ningún mensaje, sólo agrega un valor a una
tabla ya abierta. `crates/rcp-link::session` lo trata como el resto de
mandatos de sólo lectura (no cambia de fase, se acepta en cualquiera).

`crates/service::ray::build_spectrum_frame` sigue la recomendación de la
página del algoritmo — "tomarlas del mismo flujo de rayos que alimenta el
pipeline […] captura oportunista sobre el flujo vivo, sin perturbar nada" —
en vez de un modo dedicado: reutiliza el último `AssembledRadial` ensamblado
(cacheado en `crate::main` sólo para esto). La captura resulta de leer
`channels[c][bin][pulso]` transpuesto respecto al resto de este módulo: cada
*pulso* aporta una serie en tiempo rápido (a través de las `n_bins` celdas de
rango), que es la captura que promedia `welch_trace_dbm`, en vez de la serie
en tiempo lento (pulso a pulso) que usa el dominio Doppler del resto del
pipeline. El tramo cubre a la vez el canal de burst (celdas iniciales) y el
ruido de fondo (últimas celdas), tal como pide la página, sin código
adicional para seleccionarlo. Sólo atiende `channel::RX_0`: la página deja
la selección de canal como config, y el contrato v1.2 no tiene campo para
que el RCP la pida. Test de cableo
(`ray::tests::spectrum_frame_places_tone_at_correct_bin_and_level`, mismo
criterio de aceptación que el test de oráculo del crate): tono inyectado en
tiempo rápido, igual en cada pulso, aparece en el bin y nivel correctos tras
promediar. `cargo test --workspace` y `pytest contract/tests` limpios.

**Lo que esto NO resuelve**: `center_freq_hz`/`span_hz` de `spectrum_frame`
quedan en 0 — el contrato `DRx↔DSP` no expone frecuencia de muestreo ni
sintonía del NCO, y la página del algoritmo deja ese mapeo fuera de
`crates/spectrum-analyzer` por ser "mapeo de configuración, no un
algoritmo"; no hay de dónde tomarlos en este repositorio todavía, ni
siquiera con el contrato ya cerrado. `ref_level_dbm` usa
`config.receiver_gain_db` sin la calibración fina adicional que menciona la
página, que tampoco está modelada en este contrato. Sin promediado entre
`request_spectrum` sucesivos: cada mandato dispara un periodograma sobre el
radial vigente en ese instante, no una traza acumulada — el plan no pide
más que eso hoy.

**Fase 4 (varianza teórica del pulse-pair) — intento fallido documentado, contraste alternativo cerrado.**
El "Criterio de aceptación" de [pulse-pair](pulse-pair-moments.md) exige contrastar sesgo *y* desviación estándar
contra la varianza teórica de Doviak & Zrnić cap. 6 en función de (SNR, σv, M) — sin acceso al texto en este entorno
(bloqueado: ifremer, AMS journals y ResearchGate devolvieron 403 en todos los intentos, por curl y por WebFetch, con
distintos user-agents). Se probaron tres reconstrucciones antes de rendirse: (1) una fórmula de alta-SNR/banda-estrecha
pegada de una respuesta de otra IA resultó inconsistente consigo misma (no reduce a la fórmula "completa" pegada en el
mismo mensaje al tomar SNR→∞); (2) esa fórmula "completa" (con ρ(T) y términos de SNR) sí es internamente consistente
y reduce a `σv²/(2M)` en el límite banda-estrecha/alta-SNR — pero contrastada contra Monte Carlo sobre el modelo de
covarianza gaussiano ya usado en este repo, se desvía hasta 4× en banda estrecha + alta SNR, porque es la fórmula de
**pares independientes** (Miller & Rochwarger 1972), no la de **tren contiguo solapado** que implementa
`pulse_pair_moments` (M-1 pares de retardo-1 de una misma ráfaga de M pulsos) — Zrnić (1977) dedica el paper entero a
esa distinción; (3) una derivación propia de la varianza de `arg(R̂(1))` desde momentos de cuarto orden de gaussiana
compleja circular (Isserlis/Wick) se desvió hasta 20× en la dirección contraria, señal de error de álgebra, no de
modelo. El usuario aportó `5algor.pdf` (*RVP8 User's Manual*, SIGMET, agosto 2006) como material de respaldo —no es
Doviak & Zrnić, pero sí confirma independientemente que las **fórmulas de punto** de velocidad y ancho espectral de
este repo coinciden algebraicamente con las del procesador de referencia de la industria (ver
[pulse-pair](pulse-pair-moments.md) §"Contraste cruzado contra SIGMET RVP8" y
[índices de calidad](indices-de-calidad.md) §homónimo) — pero ese manual no deriva la varianza estadística del
estimador en función de M/SNR, así que no cierra el hueco de fase 4. **Sigue abierto**: el criterio de aceptación de
varianza teórica del pulse-pair necesita o bien acceso directo al capítulo 6 de Doviak & Zrnić / Zrnić (1977), o una
derivación propia hecha con más cuidado del que permitió esta sesión (la corrección "tren contiguo" de Zrnić 1977 no
es trivial). No hornear ningún margen de tolerancia numérico basado en las fórmulas descartadas arriba.

**SZ(8/64) — oráculo hecho pese a la deferencia a Stage 2, a pedido explícito; sin cablear, sin confirmar hardware.**
La página de [SZ(8/64)](sz-second-trip-recovery.md) excluye este algoritmo del paso 1 del método a propósito, condicionado
a que algún excitador de la instalación soporte modulación de fase programable pulso a pulso — condición que sigue sin
confirmarse. Se preguntó antes de escribir el notebook si convenía primero chequear esa condición con el equipo de
hardware; se decidió avanzar con el oráculo de todos modos porque el paso 1 del método no depende de hardware, sólo de
la fórmula. El paper original (Sachidananda & Zrnić 1999, JAOT) está bloqueado en este entorno con el mismo tipo de 403
ya documentado arriba para AMS/ResearchGate; la fórmula del código (`φ_k = 8πk²/64`, `ψ_0 = 0`, relación recursiva
`φ_k = ψ_{k-1} - ψ_k`) se tomó de una fuente secundaria de acceso público que la cita literalmente (Meymaris, Hubbert &
Ellis 2005, AMS) — inferencia con cita, no verificación directa contra la fuente primaria. El propio notebook
(`tools/oracles/sz_second_trip_recovery.ipynb`) hace una comprobación cruzada independiente de esa transcripción:
reproduce numéricamente la propiedad que la literatura le atribuye a la fórmula (el código dispersa la señal del trip
adyacente en exactamente 8 réplicas espectrales equiespaciadas, verificado por FFT directa del código, no supuesto).
Con esa base, demuestra el efecto citado como ventaja de la codificación — el sesgo de velocidad del trip fuerte se
mantiene dentro de 1 m/s de la verdad en todo un barrido de razón de potencias -10..10 dB, contra un error que supera
2 m/s sin codificar tan pronto el segundo trip iguala o supera al primero — e implementa una versión simplificada del
algoritmo de separación real (notch centrado en la velocidad del trip fuerte + recoherencia al trip débil), que
recupera la velocidad del trip débil con sesgo menor a 1 m/s y desviación menor a 2 m/s en el rango moderado (-5 a
-10 dB), degradándose de forma clara y medible en el caso extremo (-20 dB, desviación mayor a 5 m/s) — el mismo patrón
de curva de error frente a razón de potencias que pide el criterio de aceptación de la página, aunque sin las cifras
exactas de la fuente porque el escenario simulado es propio, no el suyo. Fuera de alcance, declarado en el propio
notebook: ancho espectral de cualquiera de los dos trips (exige "magnitude deconvolution" de Sachidananda & Zrnić 1999 /
Frush & Doviak 2002, no implementada), potencia del trip fuerte vía notch con factor de corrección, más de dos trips
solapados, ventaneo/corrección de sidelobes, e interacción con polarimetría alternante.

**Confirmado 2026-09-04: con klistrón, el excitador sí soporta fase programable pulso a pulso — la fase se sintetiza
digitalmente en FI, sin restricción de la etapa de RF.** Cierra el bloqueo de hardware que dejaba este workstream en
"sólo oráculo, sin justificación para pasar a Rust" — para la variante klistrón del Eje 1. Con magnetrón sigue sin
aplicar SZ (la vía ahí es la recuperación por fase aleatoria de [dealiasing de rango](dealiasing-de-rango.md), ya
cableada). A pedido explícito, se promovió SZ(8/64) de "Stage 2 / diferido" a compromiso real de Stage 1 para
instalación klistrón: paso 2 y paso 3 del método hechos en `crates/sz864` (construcción del código con los mismos dos
tests de la Prueba 1 del oráculo como tests unitarios, y separación de trips por notch + recoherencia, contrastada
contra las Pruebas 2 y 3 en `crates/sz864/tests/against_oracle.rs`), reutilizando sin reimplementar
`lamula_burst::correct_phase`, `lamula_moments::pulse_pair_moments`, `lamula_dual_prf::fold` y
`lamula_spectral::bin_velocity`. `cargo build`/`cargo test --workspace`/`cargo clippy -- -D warnings`/`cargo fmt --check`
en verde. **Sigue sin cablear en `crates/service::ray`**: falta que el contrato distinga esta vía de la de fase
aleatoria en magnetrón (hoy ambas comparten el mismo bit `range_dealias`, ver la decisión de arriba) y falta el canal
por el que el excitador recibe el patrón de fase a transmitir — trabajo de contrato e integración con DRx, no de
algoritmo, y no emprendido en este cambio.

**2026-09-16: mitad del trámite de contrato de arriba, hecha — la mitad que le corresponde a este proyecto.** El
contrato `DSP↔RCP` (propio, `contract/schema/dsp_rcp_v0_1.toml`) ya distingue las dos vías: `config.range_dealias`
(`u8` booleano) se reemplaza por `config.range_dealias_mode`, enumeración `NONE`/`RANDOM_PHASE`/`SZ_8_64` en el mismo
byte y misma posición de wire — `0`/`1` conservan el significado que ya tenían, así que el cambio es compatible
(`version_minor` 2→3, no `version_major`). Se agrega `capability_flag::SZ864` (bit 256) junto al `RANGE_DEALIAS`
existente (bit 16, ahora documentado explícitamente como "sólo la vía de fase aleatoria"), para que `status`/
`selftest_result` puedan anunciar la capacidad SZ(8/64) por separado de la de magnetrón. Regenerado
`contract/generated/` (Rust/Python/TS) con `tools/gen_contract.py`; actualizados los sitios que ya usaban el campo
(`crates/rcp-link/src/{session,validate,wire}.rs`, sus tests, `crates/service::ray`, `crates/service/tests/*`,
`crates/service/benches/moment_ray.rs`, `crates/contract/tests/layout.rs`, `contract/tests/test_dsp_rcp_codegen.py`).
En `crates/service::ray` el bloque de detección/marcado cross-radial ahora comprueba explícitamente
`range_dealias_mode::RANDOM_PHASE` en vez de `!= 0`: un `config` con `SZ_8_64` no entra en ese bloque (no se censura
ni se recupera nada ahí), porque ese bloque asume la vía de magnetrón (`MAGNETRON_TRANSMITTER` +
`burst_phase_correct`) y aplicarlo a una instalación SZ(8/64) sería incorrecto, no sólo incompleto. **Lo que esto NO
hace**: no cablea `crates/sz864::separate_trips` en `crates/service::ray` (ese bloque sigue viendo sólo
`uz_values`/`v_values`/`cz_values` ya reducidos a momentos, no la serie compleja cruda por pulso que `separate_trips`
necesita) y no toca el contrato `DRx↔DSP` (vendorizado, `contract/vendor/`, propiedad del proyecto DRx) para el canal
por el que el excitador recibiría el patrón de fase — eso sigue siendo trámite cross-equipo, no algoritmo, y sigue sin
emprender. `cargo build`/`cargo test --workspace` (79 test binaries, 0 fallos)/`cargo clippy --all-targets -- -D
warnings`/`cargo fmt --check` en verde; batería de contraste de codegen en Python (`contract/tests`, `.venv/bin/python
-m pytest`, 71/71) también en verde.

**Fase 4 — semilla de la puerta de rendimiento (`crates/service/benches/moment_ray.rs`), no un gate de CI todavía.**
`docs/dsp-plan.md` §10 pide "benchmark-regression gates (criterion) en los hot loops" porque el compute de estimación
de momentos, no el enlace 1GbE, es la restricción firm-real-time (§4.3, §11). Hasta este cambio no existía ningún
benchmark en el repositorio. Se agregó un target `criterion` sobre `build_moment_ray` — el hot path real, no un
micro-benchmark de una sola etapa — con un radial dual-pol representativo (todos los momentos activos, filtro de
clutter GMAP y RFI encendidos) a dos tamaños: 100 celdas de referencia y 1840 celdas (`docs/dsp-plan.md` §3.1, alcance
máximo de reflectividad, 460 km a 250 m de espaciado). Requirió separar `crates/service` en biblioteca (`src/lib.rs`,
`ray`/`config`) + binario delgado (`src/main.rs` ya no declara sus propios `mod`, usa la biblioteca): un target
`[[bench]]` sólo puede enlazar contra un target de biblioteca, nunca contra `src/main.rs`. Medido en esta sesión (no
el hardware objetivo, sin valor de referencia): ~1.45 ms/radial a 100 celdas, ~30.3 ms/radial a 1840 celdas, un único
hilo. **Esto NO es un gate**: ningún documento de este repositorio fija un presupuesto de tiempo por radial (PRF
máxima × celdas máximas no está cerrado, ver `docs/dsp-plan.md` §11), así que no hay umbral contra el que fallar
automáticamente todavía, y la CPU/SBC objetivo del Fase 0 (§8.2) sigue sin decidirse — el número de esta sesión sólo
sirve para detectar una regresión relativa entre dos ejecuciones en la misma máquina, no para certificar throughput
real. `make bench` corre el benchmark; deliberadamente fuera de `make check`/CI hasta que exista ese presupuesto.
`cargo build`/`cargo test --workspace`/`cargo clippy --all-targets -- -D warnings`/`cargo fmt --check` en verde.

**Fase 4 — inyección de fallos en `crates/simulator`, alcance Stage 1 declarado en `docs/dsp-plan.md` §3.1.**
El plan pide explícitamente que el simulador soporte "fault injection (malformed frames, dropped rays, encoder
glitches, frequency drift) for BITE testing" — hasta este cambio, `crates/simulator` no tenía nada de esto (su propio
`lib.rs` marcaba "firma de transmisor (magnetrón/coherente + burst)" como fuera de alcance). Dos módulos nuevos:

`fault` cubre las tres categorías mecánicas sobre una trama ya empaquetada por `pack_rays` — sin fórmula propia, sólo
manipulación de bytes/listas: `corrupt_frame`/`FrameFault` (magic inválido, versión no soportada, cabecera/payload
truncados, `payload_len` inflado — cada variante contrastada contra el `IngestError` real que produce
`lamula_ingest::wire::decode_ray_frame`, no sólo "no revienta"), `drop_rays` (descarta tramas por índice, para
ejercitar el conteo de `dropped_pulses` de `RadialAssembler`) e `inject_encoder_glitch` (sobrescribe
`azimuth_raw`/`elevation_raw` de una trama por offset de bytes, confirmado con un test que decodifica antes/después y
compara que sólo esos dos campos cambiaron).

`burst` cierra el hueco de "firma de transmisor" que el propio `lib.rs` señalaba: `magnetron_phase_sequence` (fase
uniforme e independiente por pulso), `drifting_phase_sequence` (fase acumulada de un perfil de frecuencia arbitrario
en el tiempo — agnóstico de si el eje es pulso a pulso para `AfcLoop` o tiempo rápido dentro de una ventana de burst
para `burst_freq_estimate`, ver su doc-comment), `apply_transmit_phase` (aplica esa fase a un canal de eco coherente,
generalizando lo que `crates/service::ray::tests::burst_phase_correction_recovers_velocity_from_magnetron_pulse_to_pulse_phase_noise`
armaba a mano) y `generate_burst` (la muestra de burst en sí, amplitud + fase + ruido térmico). Ninguna fórmula nueva
que oracular — reutiliza y contrasta contra `crates/burst` (`burst_freq_estimate`/`burst_phase_estimate`/
`correct_phase`), que ya tiene su propio oráculo; los tests de este módulo son de cableo/sanity, no de exactitud
nueva. Requirió `lamula-burst`/`lamula-ingest` como dev-dependencies nuevas de `crates/simulator` — ciclo de
dev-dependencies con `crates/ingest` (que ya depende de `lamula-simulator` en sus propios tests), explícitamente
soportado por Cargo. `cargo test --workspace`/`cargo clippy --all-targets -- -D warnings`/`cargo fmt --check` en
verde.

**Lo que esto NO resuelve (a la fecha de este cambio)**: nada consume estos generadores todavía desde un escenario de
BITE guionado de punta a punta (el plan pide "scripted weather/clutter/noise scenarios" — este cambio da las piezas,
no un guion armado); el registro de fidelidad simulador-vs-real (§10 "Simulator-fidelity register") sigue sin existir;
y clutter/multi-trip/RFI narrowband/polarización alternante siguen fuera de alcance del simulador (mismo estado que
antes de este cambio, ver el doc-comment de `crate`). **Actualizado más abajo**: el primer punto ya no aplica, ver
"Escenario guionado de BITE de punta a punta".

**Fase 4 — escenario guionado de BITE de punta a punta, y un hallazgo real al construirlo: los tres adapters de
`crates/ingest` morían ante una sola trama malformada.** Al intentar cablear el primer guion (`docs/dsp-plan.md`
§"fault injection ... for BITE testing", el hueco que dejó abierto el ítem anterior) con
`lamula_simulator::fault::corrupt_frame` alimentando `lamula_ingest::simulator::spawn`, apareció un bug de
robustez preexistente, no introducido por este cambio: `tcp::spawn`/`udp::spawn`/`simulator::spawn` propagaban el
`Result` de `decode_ray_frame` con `?`, así que una sola trama rechazada (`BadMagic`/`UnsupportedVersion`/
`UnexpectedMsgType`/`Truncated`) terminaba la tarea de ingesta entera — imposible ejercitar "fault injection... for
BITE testing" de punta a punta sin reconectar a mano después de cada falla inyectada, y en producción, una única
trama corrupta del DRx habría tumbado el enlace completo en vez de degradar una celda. Corregido en los tres
adapters: la trama rechazada se cuenta en el campo nuevo `IngestSource::malformed_frames`
(`Arc<AtomicU64>`, mismo patrón que `dropped_pulses` de `RadialAssembler` pero una capa antes, sin Status & BITE
Manager que lo publique todavía, ver el doc-comment de `crates/ingest`) y se descarta sin terminar la tarea. En TCP
esto es seguro porque el `read_exact` ya consumió del socket exactamente los bytes que el propio encabezado de esa
trama declaraba antes de intentar decodificarla, así que la sincronía de bytes para la trama siguiente no se pierde
— salvo que la corrupción caiga sobre el propio `payload_len` (`FrameFault::PayloadLenTooLarge`), caso que sigue sin
cubrirse (exigiría resincronizar buscando `MAGIC` byte a byte, no emprendido). En UDP y en el adapter `simulator` no
hay siquiera esa sutileza: un datagrama/entrada de lista es una unidad ya delimitada.

Con eso resuelto, `crates/ingest/tests/bite_scenario.rs` guiona una sola ráfaga con las tres categorías de falla a la
vez — una trama `BadMagic`, un pulso descartado (`drop_rays`) y un glitch de encoder (`inject_encoder_glitch`) sobre
uno que sobrevive — corriendo por el camino real (`simulator::spawn` → `RadialAssembler` → `pulse_pair_moments`,
misma columna vertebral que `tests/vertical_slice.rs`). Criterio de aceptación deliberadamente modesto, mismo
espíritu que `crates/service/tests/soak.rs` ("no sale infinito", no "el valor es exacto por dígito"): el radial se
completa sin pánico con exactamente las muestras que sobrevivieron, `malformed_frames` y `dropped_pulses` cuentan
las tres fallas correctamente (`dropped_pulses` no distingue "corrupta" de "descartada" — para `RadialAssembler` las
dos son igual de invisibles, un hueco de `seq`), el glitch de encoder llega intacto hasta el radial (filtrarlo es
trabajo de una capa de BITE que no existe en este workspace), y la velocidad estimada sobre la serie con dos huecos
sin rellenar sigue siendo un número finito dentro del rango de Nyquist, no basura. **Deliberadamente no incluido**:
una curva de exactitud real de momentos-bajo-falla-inyectada (sesgo en función de posición/tipo de falla) — eso es
trabajo de oráculo aparte, y su tolerancia se dejó deliberadamente floja (rango de Nyquist, no un margen ajustado)
para no hornear un número sin poder contrastarlo, mismo principio que la fase 4 ya aplicó al abandonar la varianza
teórica de pulse-pair sin acceso al capítulo 6. **Verificado en sesión posterior, una vez reparado el toolchain Rust
del entorno** (rustup tenía instalado por error un toolchain `x86_64-unknown-linux-gnu` en un host `aarch64` real sin
qemu; reinstalado nativo): `cargo build`/`cargo test --workspace`/`cargo clippy --all-targets -- -D warnings`/
`cargo fmt --check` en verde, incluido `bite_scenario.rs`.

**Lo que esto sigue sin resolver**: el registro de fidelidad simulador-vs-real y la cobertura de
clutter/multi-trip/RFI narrowband/polarización alternante en el simulador, exactamente igual que antes (ver arriba);
y drift de frecuencia (`lamula_simulator::burst::drifting_phase_sequence`) sigue sin tener su propio escenario
guionado — este cambio cubrió las tres categorías del párrafo original de fault injection (tramas malformadas,
pulsos perdidos, glitch de encoder), no la cuarta (deriva de frecuencia/AFC), que por su propia naturaleza necesita
varios radiales sucesivos para observarse converger, no uno solo.

**Fase 4 — semilla de endurance/soak (`crates/service/tests/soak.rs`), no el soak real que pide el plan.**
`docs/dsp-plan.md` §10 pide "endurance/soak runs (long unattended processing at worst-case PRF/range)" en fase 4. No
hay hardware ni entorno de horas de duración en esta sesión, así que se agregó lo reproducible: una prueba que corre
2000 radiales sintéticos variados (gate count 1–50, filtro de clutter GMAP/ninguno alternando, RFI activo una vuelta
de cada tres) de punta a punta por el pipeline real — `pack_rays` → `decode_ray_frame` → `RadialAssembler` (la misma
ingesta que usa `crates/ingest::tcp`) → `build_moment_ray` —, comprobando que `ray_seq` es monótono y que ningún
momento publicado sale infinito (la censura explícita produce NaN a propósito; eso no es una falla). Corre en ~8 s
como parte de `cargo test`/`make test`, no aparte.

**Lo que esto NO prueba**: horas de duración real, la PRF/rango efectivamente peor caso de un hardware objetivo que
Fase 0 todavía no decide, fugas de memoria (sin herramienta de instrumentación en este entorno), ni el binario real
por TCP bajo carga sostenida (`tests/end_to_end.rs` ya cubre el binario real, pero a una sola conexión corta). El
propio test lo documenta en su doc-comment para que quede claro qué reemplaza y qué no.

**Fase 4 — empaquetado offline/air-gapped, alcance Stage 1 declarado en `docs/dsp-plan.md` §3.1.** El plan pide "an
offline, air-gapped installer for the Linux SBC (systemd service); reproducible offline build" — no existía ningún
artefacto de esto. Se agregaron tres piezas, documentadas en `docs/despliegue.md` (página nueva, con nav propio en
`mkdocs.yml`): `tools/offline_build.sh` (dos fases separadas — `vendor` con red genera el árbol vendorizado y la
config de Cargo que apunta a él; `build` compila `--offline --release` exclusivamente desde ahí — las dos verificadas
de punta a punta en esta sesión contra este `Cargo.lock`), `packaging/lamula-dsp.service` (unidad systemd con
`Restart=on-failure` — coherente con que `crates/ingest`/`crates/rcp-link` ya reconectan solos, así que si el proceso
termina fue un fallo real — y endurecimiento básico gratuito: `NoNewPrivileges`, `ProtectSystem=strict`, sin
`ReadWritePaths` porque este binario no escribe nada en disco hoy) y `packaging/lamula-dsp.env.example` (las nueve
variables obligatorias de `crates/service::config::ServiceConfig`, documentadas una por una con el porqué de que
ninguna tenga valor por defecto — consolida en un solo lugar un listado que hasta ahora vivía repartido entre el
doc-comment de `config.rs` y cada punto de uso).

**Lo que esto NO resuelve**: no hay instalador real (`.deb`/`.rpm`/imagen de SBC), no hay cross-compilación ARM
(Fase 0 no decidió CPU/SBC objetivo todavía), y el archivo de I/Q crudo de investigación (mismo §3.1) sigue sin
cablear en `crates/service` — la unidad systemd lo señala explícitamente en vez de fingir que ya existe.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., Academic Press, 1993 — referencia canónica transversal a todo el conjunto.
- Bringi, V. N. & Chandrasekar, V., *Polarimetric Doppler Weather Radar: Principles and Applications*, Cambridge University Press, 2001 — referencia canónica de la parte polarimétrica.
- [Py-ART](https://github.com/ARM-DOE/pyart), [wradlib](https://github.com/wradlib/wradlib), [LROSE/RadX](https://github.com/NCAR/lrose-core) — implementaciones abiertas usadas como oráculo y contraste en el paso 1 del método.
