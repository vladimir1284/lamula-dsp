# Dealiasing Dual-PRF

## Qué resuelve

La velocidad radial que un radar Doppler puede medir sin ambigüedad está acotada por la velocidad de Nyquist, `v_N = λ / (4·PRT)`, donde λ es la longitud de onda y PRT el período entre pulsos. Para PRTs típicos de vigilancia meteorológica de largo alcance, `v_N` suele quedar entre 8 y 20 m/s — muy por debajo de velocidades radiales reales en convección severa. Toda velocidad que exceda `v_N` se "pliega" (aliasing): se mide un valor erróneo desplazado en múltiplos de `2·v_N`. Subir el PRF para ampliar `v_N` reduce a su vez el rango no ambiguo (`r_max = c·PRT/2`), así que hay un compromiso directo rango-velocidad que un único PRF no puede resolver por sí solo.

## Cómo funciona dual-PRF/PRT

La técnica **dual-PRF** (o dual-PRT) transmite ráfagas alternando entre dos PRFs distintos (típicamente en razón simple, por ejemplo 2:3 o 3:4). Cada PRF produce su propia velocidad de Nyquist y, por tanto, su propio patrón de aliasing; pero como los dos PRFs comparten el mismo valor físico de velocidad radial verdadera, comparar la velocidad medida por ambos permite resolver de forma unívoca (dentro de un rango extendido) cuál era la velocidad real antes del plegado — de forma análoga al teorema chino del resto aplicado a dos "relojes" de período distinto. El rango de velocidad no ambigua efectivo se extiende aproximadamente por el factor de la razón entre PRFs (p.ej. una razón 4:5 puede extender `v_N` hasta 4-5 veces el valor de un solo PRF).

El algoritmo de "unfolding" o desdoblado debe: (1) estimar la velocidad con cada sub-conjunto de pulsos de cada PRF, (2) para cada celda, probar los múltiplos de plegado consistentes con ambos valores medidos y seleccionar el que los reconcilia, y (3) aplicar continuidad espacial (comparación con celdas vecinas ya resueltas) para descartar soluciones ambiguas en zonas de bajo SNR o transición. Esta clase de algoritmos está descrita extensamente en la literatura operativa de redes Doppler dual-PRF, en particular en los trabajos de Joe & May sobre la implementación operativa del esquema dual-PRF en redes de radares meteorológicos, y en Holleman & Beekhuis sobre validación y corrección de errores de dealiasing dual-PRF en radares operativos europeos.

## Relevancia para LAMULA DSP

El módulo de **Dealiasing** del pipeline (ver el plan de LAMULA DSP) implementará el modo dual-PRF como mecanismo primario de extensión de velocidad no ambigua, con soporte para razones configurables de PRF y continuidad espacial post-proceso — capacidad que los procesadores de gama alta ofrecen como modo de escaneo seleccionable junto al modo staggered-PRT.

## Referencias abiertas / implementaciones libres

- Joe, P. & May, P. T. (2003), "Correction of Dual PRF Velocity Errors for Operational Doppler Weather Radars", *Journal of Atmospheric and Oceanic Technology*.
- Holleman, I. & Beekhuis, H. (2003), "Analysis and Correction of Dual PRF Velocity Data", *Journal of Atmospheric and Oceanic Technology*.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2nd ed., 1993 — fundamentos de aliasing de velocidad y técnicas de extensión de rango no ambiguo.
- [Py-ART](https://github.com/ARM-DOE/pyart) — el módulo `pyart.correct` implementa varios algoritmos de dealiasing de velocidad (incluyendo región-based y four-dimensional dealiasing) sobre datos reales, útil como referencia de post-proceso incluso cuando el unfolding dual-PRF ya se resolvió en el DSP.
- [LROSE / RadX](https://github.com/NCAR/lrose-core) — incluye utilidades de procesamiento de series temporales dual-PRF/dual-PRT en su motor de radar de código abierto.
