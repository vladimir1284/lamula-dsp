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
| 3 → M3 (W19–27) | [Covarianzas polarimétricas](polarimetria-covarianzas.md); [KDP](kdp-estimacion.md); [calibración polarimétrica](calibracion-polarimetrica.md); [dealiasing de rango](dealiasing-de-rango.md); [analizador de espectro de FI](analizador-espectro-fi.md) |
| 4 → M4 (W28–34) | Validación de exactitud contra varianza teórica y regresión Vesta; gates de rendimiento |

**Estado del paso 1 del método (oráculo en Python).** Completo para todo el
trabajo de fase 0 a fase 3: cada algoritmo de la tabla —salvo el kernel
numérico, que no tiene fórmula propia que oracular, y salvo
[SZ(8/64)](sz-second-trip-recovery.md), diferido a Stage 2 por su propia
página— tiene su notebook en `tools/oracles/`, enlazado desde la página del
algoritmo correspondiente, ejecutable de punta a punta con `make
test-oracles`. El paso 2 (implementación en Rust) y el paso 3 (test de
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
velocidad y ancho espectral; el estimador espectral como modo alternativo
queda pendiente). Están pendientes en el resto de algoritmos de fase 2 en
adelante.

**Dependencia que mueve una pieza de fase.** El plan sitúa el burst/AFC en la
fase 2, junto al resto de la suite Doppler. Si la instalación es de magnetrón,
la corrección de fase es prerrequisito duro del pulse-pair —sin ella la serie
temporal no es coherente y el estimador de velocidad no significa nada— y sube a
la fase 1. Por eso aparece en la fase 1 de la tabla: es el caso que cubre las
dos configuraciones. Con transmisor coherente la etapa se reduce a monitor de
potencia y puede quedarse donde el plan la puso.

## Decisiones abiertas

Tres puntos que no se resuelven documentando, sino decidiendo. Se registran aquí
para que no se pierdan.

**`range_dealias` sin SZ.** El contrato ofrece recuperación de trip múltiple en
v0.1 y la página de [SZ(8/64)](sz-second-trip-recovery.md) la difiere a Stage 2.
La postura propuesta es que Stage 1 declare el bit según el hardware: con
magnetrón hay recuperación real por fase aleatoria, con transmisor coherente y
sin codificación programable sólo hay detección y marcado. Está desarrollado en
[dealiasing de rango](dealiasing-de-rango.md).

**Modo de polarización de la instalación.** El enum `moment_kind` promete LDR, y
LDR sólo existe en modo alternante o con un canal cruzado dedicado. El DSP lo
resuelve por capacidades, pero el equipo del DRx tiene que fijar qué canales
físicos entrega el `channel_mask` en cada configuración soportada.

**Qué significa exactamente CZ.** Por herencia de Vesta/Sigmet, CZ es la
reflectividad tras el filtro de clutter y UZ la reflectividad sin filtrar; el
esquema del contrato no lo dice con esas palabras. Si CZ debiera incluir además
corrección de atenuación, hace falta un algoritmo que hoy no está en ningún plan
(Z-PHI, Testud et al. 2000), y eso es alcance nuevo, no una aclaración de
documentación.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., Academic Press, 1993 — referencia canónica transversal a todo el conjunto.
- Bringi, V. N. & Chandrasekar, V., *Polarimetric Doppler Weather Radar: Principles and Applications*, Cambridge University Press, 2001 — referencia canónica de la parte polarimétrica.
- [Py-ART](https://github.com/ARM-DOE/pyart), [wradlib](https://github.com/wradlib/wradlib), [LROSE/RadX](https://github.com/NCAR/lrose-core) — implementaciones abiertas usadas como oráculo y contraste en el paso 1 del método.
