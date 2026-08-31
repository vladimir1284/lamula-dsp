# Staggered-PRT

> **Oráculo en Python**: [`tools/oracles/staggered_prt.ipynb`](../../tools/oracles/staggered_prt.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/staggered-prt`: velocidades pulse-pair sobre las dos subsecuencias `T1`/`T2` de la misma ráfaga, contrastada numéricamente contra el oráculo en `crates/staggered-prt/tests/against_oracle.rs`. El desdoblado reutiliza sin reimplementar el mecanismo de teorema chino del resto de `crates/dual-prf` — la propia página lo señala como "exactamente el mismo mecanismo", sólo cambia de dónde salen `v1`/`v2`. El filtrado de clutter en muestreo escalonado (Sachidananda & Zrnić 2000) tiene ahora su propio oráculo, [`tools/oracles/staggered_prt_clutter_sz2000.ipynb`](../../tools/oracles/staggered_prt_clutter_sz2000.ipynb), e implementación Rust en el mismo crate (`sz2000_clutter_filter`, `reflectivity_estimate`): descompone la ráfaga en las dos subsecuencias uniformes que la componen y aplica un notch por subsecuencia, alcance de Stage 1 declarado — no la reconstrucción gaussiana (GMAP) de `crates/clutter`, cuya ausencia se mide y declara en el propio oráculo como pérdida de señal cuando la velocidad verdadera cae en la banda de notch.

## Qué resuelve

Es la segunda vía para romper el compromiso entre rango no ambiguo y velocidad
no ambigua, alternativa al [dual-PRF](dual-prf-dealiasing.md). La diferencia
está en el grano: dual-PRF alterna el PRF entre radiales o entre bloques de
pulsos, mientras que staggered-PRT alterna el periodo entre pulsos consecutivos
*dentro* del mismo radial, según un patrón T1, T2, T1, T2…

Esa diferencia tiene dos consecuencias prácticas de signo opuesto. A favor: como
las dos muestras que se comparan están separadas por milisegundos y no por el
tiempo de un radial completo, el eco no ha cambiado entre ellas, y desaparece el
error de dealiasing por decorrelación que sufre el dual-PRF en presencia de
cizalladura o turbulencia fuerte. En contra: el filtrado de clutter se vuelve
mucho más difícil, porque un muestreo no uniforme no tiene una FFT ordinaria
detrás y las técnicas espectrales estándar dejan de aplicarse tal cual.

## Cómo funciona

Con dos periodos T1 y T2 en razón simple —2/3 y 3/4 son las habituales— se
calculan las autocovarianzas a los dos retardos correspondientes. Cada una da
una estimación de velocidad plegada con su propia velocidad de Nyquist. La
diferencia de las dos fases es proporcional a la velocidad verdadera con una
velocidad de Nyquist extendida igual a la que correspondería al periodo
diferencia `T2 − T1`, que es mucho menor que cualquiera de los dos y por tanto da
un intervalo no ambiguo mucho mayor. En la práctica no se usa la diferencia
cruda, que es muy ruidosa, sino como índice para elegir el número de pliegues
correcto de la estimación individual más precisa: se calcula el par de enteros
que reconcilia las dos medidas mediante una tabla de búsqueda indexada por la
diferencia de fases, y se aplica ese desdoblado a la estimación de mejor
varianza. Torres, Dubel & Zrnić (2004) describen esa implementación con la tabla
de reglas y su análisis de errores.

**El problema del clutter.** Con muestreo uniforme, el clutter está en cero
Hertz y se filtra en frecuencia. Con muestreo escalonado, el espectro de una
señal muestreada de forma no uniforme presenta réplicas que ensucian la banda, y
un filtro de muesca ingenuo destruye señal meteorológica que no debería tocar.
La solución publicada por Sachidananda & Zrnić (2000) separa el problema:
descompone la serie escalonada en las dos subsecuencias uniformes que la
componen, filtra en un dominio donde el clutter sí está localizado, y reconstruye.
Es el trabajo más pesado de esta técnica y la razón principal por la que el
staggered-PRT es más caro de implementar bien que el dual-PRF.

## Configuraciones cubiertas

Requiere que el DRx sepa generar el patrón de disparo escalonado —el contrato
`DRx↔DSP` lo permite mediante los cuatro pares de `trigger_delay_N` y
`trigger_width_N`— y no depende del tipo de transmisor ni de la polarimetría. En
configuración polarimétrica alternante, sin embargo, la combinación de
alternancia de polarización y alternancia de periodo genera un patrón de
muestreo cuya estadística hay que analizar antes de comprometerla: no se
recomienda ofrecer las dos a la vez en Stage 1, y la restricción se declara vía
`dealias_mask` y `capability_flags`.

## Parámetros del contrato que consume

De `config`: `dealias_mode = staggered_prt`, y `prf_ratio_num`/`prf_ratio_den`
para la razón T1:T2 —los mismos campos que el dual-PRF, con el significado que
fija el modo—. Publica en cada radial la `nyquist_velocity` extendida resultante
y marca `ray_flag.dealias_failed` cuando el desdoblado no converge. La
disponibilidad se declara en `dealias_mask`; si se pide y no está, el rechazo es
`dealias_unsupported`.

## Criterio de aceptación

Sobre una matriz de escenarios con velocidad verdadera barrida más allá de la
Nyquist de cada periodo individual y hasta la Nyquist extendida, la velocidad
recuperada debe coincidir con la verdad-terreno; el criterio se expresa como
tasa de aciertos del número de pliegues, no como error medio, porque un fallo de
desdoblado es un error de varios múltiplos de la Nyquist y promediarlo con
aciertos oculta el problema. Esa tasa debe medirse en función de la SNR y del
ancho espectral, y degradarse de forma suave. Con clutter inyectado, la
reflectividad y la velocidad tras el filtrado escalonado deben mantenerse dentro
del mismo margen que el caso uniforme, que es la prueba de que el filtro
específico está bien implementado.

## Coste de cómputo

Las dos autocovarianzas cuestan aproximadamente el doble que el pulse-pair
ordinario, más la búsqueda en tabla, que es despreciable. El filtrado de
clutter añade dos FFT/IFFT de `M/2` puntos por celda —una por subsecuencia—,
del mismo orden que el estimador espectral; la variante GMAP, si se
implementa más adelante, costaría más por el ajuste de mínimos cuadrados en
cada subsecuencia.

## Referencias abiertas / implementaciones libres

- Torres, S. M., Dubel, Y. F. & Zrnić, D. S. (2004), «Design, Implementation, and Demonstration of a Staggered PRT Algorithm for the WSR-88D», *Journal of Atmospheric and Oceanic Technology*.
- Sachidananda, M. & Zrnić, D. S. (2000), «Clutter Filtering and Spectral Moment Estimation for Doppler Weather Radars Using Staggered Pulse Repetition Time (PRT)», *Journal of Atmospheric and Oceanic Technology*.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — fundamentos de ambigüedad rango-velocidad.
- [LROSE](https://github.com/NCAR/lrose-core) — incluye utilidades de procesamiento de series temporales con PRT escalonado, la referencia abierta más cercana a una implementación completa.
