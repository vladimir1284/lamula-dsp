# Pulse-Pair y Estimación de Momentos

> **Oráculo en Python**: [`tools/oracles/pulse_pair_moments.ipynb`](../../tools/oracles/pulse_pair_moments.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/moments`: potencia, velocidad y ancho espectral por pulse-pair, contrastada numéricamente contra el oráculo en `crates/moments/tests/against_oracle.rs`. El estimador espectral (FFT + ajuste) como modo alternativo queda pendiente (sin oráculo todavía).

## Qué resuelve

Un radar meteorológico Doppler transmite una ráfaga de pulsos por rayo y recibe, para cada celda de rango, una serie temporal de muestras complejas I/Q (una por pulso). De esa serie hay que extraer tres cantidades físicas por celda: la potencia recibida (de la que se deriva la reflectividad Z), la velocidad radial media (efecto Doppler) y el ancho espectral (dispersión de velocidades dentro del volumen de resolución, ligada a turbulencia y cizalladura). El método clásico y computacionalmente más económico para estimar estas tres cantidades es el **estimador pulse-pair**, también llamado estimador de autocovarianza de retardo 1.

## Cómo funciona

El estimador se basa en la función de autocovarianza de la serie temporal `s(1), s(2), ..., s(M)` (M pulsos por rayo). La potencia total se estima como el promedio de `|s(m)|²`; la velocidad radial se obtiene de la fase del promedio de `s(m) · s*(m+1)` (autocovarianza a retardo 1), escalada por la longitud de onda y el período entre pulsos (PRT); y el ancho espectral se deriva de la relación entre las magnitudes de la autocovarianza a retardo 0 y retardo 1, bajo un modelo de espectro gaussiano. Toda la formulación cerrada de estos tres estimadores, junto con su varianza teórica en función del número de muestras independientes, está desarrollada en detalle en Doviak & Zrnić, *Doppler Radar and Weather Observations* (2ª ed., 1993), capítulo 6, que sigue siendo la referencia canónica del campo. El estimador de velocidad específico usado casi universalmente en la industria proviene de Zrnić (1977), "Spectral Moment Estimates from Correlated Pulse Pairs", el trabajo que le da nombre a la técnica.

La ventaja del pulse-pair frente a una FFT completa del espectro Doppler es el costo: requiere solo un producto complejo por par de pulsos consecutivos (O(M) en vez de O(M log M)), lo que en los años 80-90 fue decisivo y hoy sigue siendo relevante para procesar en tiempo real cientos de miles de celdas por segundo en hardware embebido. Su limitación principal es que asume un espectro Doppler unimodal aproximadamente gaussiano; en presencia de clutter superpuesto a la señal meteorológica (dos modos en el espectro) el estimador se sesga, razón por la cual el filtrado de clutter (ver [GMAP](gmap-clutter-filtering.md)) debe aplicarse *antes* de la estimación de momentos, no después.

## Relevancia para LAMULA DSP

El **Moment Estimator** del pipeline de LAMULA DSP (ver el plan de LAMULA DSP, sección de arquitectura) implementará este estimador como algoritmo primario, con la variante de estimación espectral (FFT + ajuste) reservada como modo alternativo de mayor costo para escenarios donde la relación señal/clutter lo justifique — la misma dualidad que documentan los procesadores comerciales de gama alta.

## Configuraciones cubiertas

Independiente de la polarimetría —se aplica por canal— con una salvedad: en modo
alternante, las muestras de cada canal están separadas por dos PRT en vez de uno,
así que la velocidad de Nyquist efectiva es la mitad y el retardo de la
autocovarianza que se usa no es el mismo. Respecto del transmisor, la
dependencia es dura: con magnetrón la serie temporal **no es coherente** hasta
que se le ha aplicado la [corrección de fase por burst](burst-fase-afc.md), y
sobre una serie sin corregir el estimador de velocidad devuelve ruido uniforme.
Con transmisor coherente esa etapa es opcional. Ésa es la razón de que el burst
suba a la fase 1 del [plan de trabajo](roadmap.md).

## Parámetros del contrato que consume

De `config`: `estimator = pulse_pair`, `n_pulses` como número de muestras,
`wavelength_m` y `prf_hz` para escalar la velocidad, `noise_floor_dbm` para la
resta de ruido, y los cuatro umbrales de censura. Publica `uz`, `cz`, `v` y `w`,
y las cantidades intermedias que alimentan los
[índices de calidad](indices-de-calidad.md).

## Criterio de aceptación

Malla de (SNR, σv, M) con N realizaciones independientes por punto: el sesgo y la
desviación estándar de los tres momentos deben quedar dentro del margen declarado
respecto de la varianza teórica de Doviak & Zrnić, capítulo 6. El caso límite que
más importa vigilar es σv grande con M pequeño, donde el estimador de ancho
espectral se satura, y σv muy pequeño, donde la fórmula del logaritmo se vuelve
numéricamente delicada; los dos extremos deben tener comportamiento declarado y
no un NaN.

## Contraste cruzado contra SIGMET RVP8

La fase 4 del [plan de trabajo](roadmap.md) pide contrastar sesgo y varianza
contra la curva teórica de Doviak & Zrnić cap. 6 — sin acceso al texto en este
entorno, ese contraste sigue abierto (ver roadmap). Como verificación
independiente de que las *fórmulas de los estimadores* (no su varianza) están
bien portadas, se contrastaron algebraicamente contra el *RVP8 User's Manual*
de SIGMET (agosto de 2006, cap. 5 "Processing Algorithms") — no es el paper
original, pero es la documentación pública del procesador de referencia de la
industria (WSR-88D ORDA y la mayoría de radares operativos usan IRIS/RVP8 o un
derivado), y expone las fórmulas cerradas que ese procesador realmente calcula.

- **Velocidad.** El manual define `R1 = (1/(M-1))·Σ s'*_n·s'_{n+1}` (conjuga la
  muestra *anterior*) y `V = (λ/4πτs)·arg(R1)`. Este repo calcula
  `r1 = s_n·conj(s_{n+1})` (conjuga la *posterior*), es decir
  `r1 = conj(R1_RVP8)`, así que `arg(r1) = -arg(R1_RVP8)`. Sustituyendo,
  `V = -(λ/4πτs)·arg(r1)` — exactamente `PulsePairEstimate::velocity_mps`
  (`crates/moments/src/pulse_pair.rs`). Coinciden en valor; sólo difiere la
  convención de qué muestra se conjuga.
- **Ancho espectral, algoritmo "rápido" R0,R1.** El manual da
  `Var = -2·ln(SQI)` normalizada al intervalo de Nyquist `[-π,π]`, con
  `W = √Var/π` (normalizado a `[-1,1]`) y por tanto, en unidades físicas,
  `W_mps = (λ/4πτs)·√(-2·ln(SQI))`. Reescribiendo el coeficiente,
  `(λ/4πτs)·√2 = λ/(2√2·π·τs)`, que es exactamente el coeficiente que usa
  `pulse_pair_moments`: `wavelength_m/(2π·prt_s·√2) · √(-ln(ratio))`. La única
  diferencia real es el denominador de `ratio`: el manual usa `SQI = |R1|/R0`
  (con `R0` sin restar ruido, válido sólo cuando `SNR/(SNR+1) ≈ 1`, como el
  propio manual aclara), mientras que este repo usa `|R1|/S` con `S` ya
  corregido de ruido — una generalización que no exige SNR alto para ser
  válida, no una fórmula distinta.
- **SQI** coincide de forma literal (`|R1|/R0`, misma definición, ver
  [índices de calidad](indices-de-calidad.md)). **CCOR** coincide en su forma
  general (`10·log10(filtrada/total)`); el manual expone además una variante
  algebraica en términos de `CSR` que este repo no reproduce porque no aporta
  nada nuevo, es la misma cantidad. **SIG** persigue la misma cantidad física
  (SNR tras clutter) pero por un camino algebraico distinto — el manual la
  arma como `10·log10((T0−N)/N) + CCOR` sobre `T0`/`R0` sin filtrar
  explícitamente el ruido primero; este repo la calcula directamente sobre
  `s_linear` (ya filtrado de clutter y de ruido, aguas arriba). No son la
  misma expresión algebraica, pero sí describen la misma cantidad si `S` (este
  repo) y `R0−N` con la corrección de `CCOR` (RVP8) se calculan sobre la misma
  serie ya filtrada.
- **Hueco encontrado, no cerrado por este contraste**: el manual también
  documenta un algoritmo "preciso" `R0,R1,R2` para el ancho espectral
  (`Var = (2/3)·ln(|R1|/|R2|)`, válido a SNR bajo, 0–5 dB) que este repo **no
  implementa** — `pulse_pair_moments` sólo calcula `R0` y `R1`. Queda como
  ítem de roadmap aparte, no bloqueante (el algoritmo `R0,R1` ya cubierto es
  el que el propio manual recomienda para SNR alto, el caso típico).

Este contraste no reemplaza la varianza teórica de fase 4 — confirma que las
fórmulas de punto (no de dispersión estadística) coinciden con las que usa un
procesador de referencia de la industria.

## Coste de cómputo

Un producto complejo y dos acumulaciones por muestra: O(M) por celda y canal, la
opción más barata del conjunto. Es el bucle caliente principal del pipeline junto
con la corrección de fase, y el candidato natural a SIMD. Debe calcular en la
misma pasada R(0), R(1) y las potencias por canal que necesita la
[polarimetría](polarimetria-covarianzas.md), para no recorrer los datos dos
veces.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2nd ed., Academic Press, 1993 — capítulo 6 (formulación completa de los estimadores).
- Zrnić, D. S. (1977), "Spectral Moment Estimates from Correlated Pulse Pairs", *IEEE Transactions on Aerospace and Electronic Systems*.
- [Py-ART](https://github.com/ARM-DOE/pyart) — el módulo de lectura/procesamiento de momentos de Py-ART (ARM Radar Toolkit, NASA/DOE) implementa y documenta estimadores de momentos sobre datos de radar reales en Python, útil como referencia de validación numérica.
- [LROSE / RadX](https://github.com/NCAR/lrose-core) (NCAR) — el motor de procesamiento de series temporales de LROSE incluye una implementación de referencia en C++ del estimador pulse-pair y del estimador espectral, con código abierto auditable.
- SIGMET, Inc., *RVP8 User's Manual*, agosto de 2006, cap. 5 "Processing Algorithms" — documentación pública del procesador comercial de referencia; usado arriba para contraste algebraico de las fórmulas de velocidad y ancho espectral.
