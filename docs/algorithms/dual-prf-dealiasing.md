# Dealiasing Dual-PRF

## Qué resuelve

La velocidad radial que un radar Doppler puede medir sin ambigüedad está acotada por la velocidad de Nyquist, `v_N = λ / (4·PRT)`, donde λ es la longitud de onda y PRT el período entre pulsos. Para PRTs típicos de vigilancia meteorológica de largo alcance, `v_N` suele quedar entre 8 y 20 m/s — muy por debajo de velocidades radiales reales en convección severa. Toda velocidad que exceda `v_N` se "pliega" (aliasing): se mide un valor erróneo desplazado en múltiplos de `2·v_N`. Subir el PRF para ampliar `v_N` reduce a su vez el rango no ambiguo (`r_max = c·PRT/2`), así que hay un compromiso directo rango-velocidad que un único PRF no puede resolver por sí solo.

## Cómo funciona dual-PRF/PRT

La técnica **dual-PRF** (o dual-PRT) transmite ráfagas alternando entre dos PRFs distintos (típicamente en razón simple, por ejemplo 2:3 o 3:4). Cada PRF produce su propia velocidad de Nyquist y, por tanto, su propio patrón de aliasing; pero como los dos PRFs comparten el mismo valor físico de velocidad radial verdadera, comparar la velocidad medida por ambos permite resolver de forma unívoca (dentro de un rango extendido) cuál era la velocidad real antes del plegado — de forma análoga al teorema chino del resto aplicado a dos "relojes" de período distinto. El rango de velocidad no ambigua efectivo se extiende aproximadamente por el factor de la razón entre PRFs (p.ej. una razón 4:5 puede extender `v_N` hasta 4-5 veces el valor de un solo PRF).

El algoritmo de "unfolding" o desdoblado debe: (1) estimar la velocidad con cada sub-conjunto de pulsos de cada PRF, (2) para cada celda, probar los múltiplos de plegado consistentes con ambos valores medidos y seleccionar el que los reconcilia, y (3) aplicar continuidad espacial (comparación con celdas vecinas ya resueltas) para descartar soluciones ambiguas en zonas de bajo SNR o transición. Esta clase de algoritmos está descrita extensamente en la literatura operativa de redes Doppler dual-PRF, en particular en los trabajos de Joe & May sobre la implementación operativa del esquema dual-PRF en redes de radares meteorológicos, y en Holleman & Beekhuis sobre validación y corrección de errores de dealiasing dual-PRF en radares operativos europeos.

## Relevancia para LAMULA DSP

El módulo de **Dealiasing** del pipeline (ver el plan de LAMULA DSP) implementará el modo dual-PRF como mecanismo primario de extensión de velocidad no ambigua, con soporte para razones configurables de PRF y continuidad espacial post-proceso — capacidad que los procesadores de gama alta ofrecen como modo de escaneo seleccionable junto al modo staggered-PRT.

## Configuraciones cubiertas

Independiente del tipo de transmisor. Con polarimetría alternante hay que
analizar la interacción antes de ofrecer las dos cosas a la vez: la alternancia
de polarización ya reduce a la mitad las muestras por canal, y combinarla con
alternancia de PRF deja bloques de muestras demasiado cortos para una estimación
estable. La restricción, si se adopta, se declara en `dealias_mask` y
`capability_flags` en vez de dejarse a que el operador descubra que la
combinación no funciona.

## Parámetros del contrato que consume

De `config`: `dealias_mode = dual_prf` y la razón `prf_ratio_num`/`prf_ratio_den`.
Publica por radial la `nyquist_velocity` extendida, el `prf_hz` medio —el
contrato lo documenta explícitamente como la media en dual-PRF— y la bandera
`ray_flag.dealias_failed` cuando el desdoblado no converge. La disponibilidad se
declara en `dealias_mask`; el rechazo es `dealias_unsupported`.

## Criterio de aceptación

El criterio es la **tasa de acierto del número de pliegues**, no el error medio
de velocidad: un fallo de desdoblado es un error de varios múltiplos de la
Nyquist, y promediarlo con los aciertos produce una cifra que no significa nada.
Esa tasa se mide barriendo velocidad verdadera hasta la Nyquist extendida, SNR y
—esto es específico del dual-PRF— cizalladura, porque el método compara medidas
separadas por el tiempo de un radial y su punto débil es justamente que el eco
haya cambiado entre las dos. La corrección por continuidad espacial se evalúa por
separado: cuántos fallos aislados recupera y cuántos aciertos estropea.

## Coste de cómputo

Despreciable: dos estimaciones pulse-pair que ya se hacen, más una comparación y
una búsqueda en tabla por celda. La continuidad espacial requiere acceso a celdas
vecinas ya resueltas, lo que impone un orden de recorrido y un pequeño buffer de
contexto, pero sigue siendo lineal.

## Referencias abiertas / implementaciones libres

- Joe, P. & May, P. T. (2003), "Correction of Dual PRF Velocity Errors for Operational Doppler Weather Radars", *Journal of Atmospheric and Oceanic Technology*.
- Holleman, I. & Beekhuis, H. (2003), "Analysis and Correction of Dual PRF Velocity Data", *Journal of Atmospheric and Oceanic Technology*.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2nd ed., 1993 — fundamentos de aliasing de velocidad y técnicas de extensión de rango no ambiguo.
- [Py-ART](https://github.com/ARM-DOE/pyart) — el módulo `pyart.correct` implementa varios algoritmos de dealiasing de velocidad (incluyendo región-based y four-dimensional dealiasing) sobre datos reales, útil como referencia de post-proceso incluso cuando el unfolding dual-PRF ya se resolvió en el DSP.
- [LROSE / RadX](https://github.com/NCAR/lrose-core) — incluye utilidades de procesamiento de series temporales dual-PRF/dual-PRT en su motor de radar de código abierto.
