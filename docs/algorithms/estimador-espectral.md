# Estimador espectral de momentos

> **Oráculo en Python**: [`tools/oracles/estimador_espectral.ipynb`](../../tools/oracles/estimador_espectral.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust pendiente.

## Qué resuelve

El [pulse-pair](pulse-pair-moments.md) es barato y suficiente cuando el espectro
Doppler tiene un solo modo aproximadamente gaussiano. Cuando no lo tiene —clutter
residual superpuesto a la señal, dos poblaciones de dispersores con velocidades
distintas en el mismo volumen, interferencia de banda estrecha, eco de segundo
trip— el pulse-pair devuelve una media ponderada de todo lo que hay en la celda y
la reporta como si fuera un eco único. El estimador espectral resuelve el
espectro completo antes de decidir qué parte es la señal meteorológica, a cambio
de un coste computacional mayor.

Además, es el estimador que hace falta si se quiere que el filtrado de clutter y
la estimación de momentos compartan el mismo dominio: [GMAP](gmap-clutter-filtering.md)
ya trabaja en frecuencia, y estimar los momentos allí mismo ahorra la IFFT de
vuelta.

## Cómo funciona

Se calcula el periodograma de la serie de M pulsos de la celda, previa
multiplicación por una ventana. La elección de ventana no es cosmética: el
clutter de tierra puede estar 50 o 60 dB por encima de la señal meteorológica, y
las colas espectrales de una ventana rectangular derraman esa potencia sobre
todo el espectro y enterran la señal. Una ventana con lóbulos laterales
suficientemente bajos —Hann como caso base, Blackman o una ventana de Chebyshev
cuando el clutter es extremo— es lo que hace viable la separación, al precio de
ensanchar el lóbulo principal y de reducir el número efectivo de muestras
independientes.

Del espectro resultante se estiman los tres momentos por su definición directa:
la potencia es la suma de las líneas espectrales atribuidas a la señal, la
velocidad media es el primer momento normalizado de esa distribución y el ancho
espectral es la raíz del segundo momento central. La parte delicada es «las
líneas atribuidas a la señal»: hay que descartar las líneas de ruido —usando el
umbral objetivo descrito en [ruido y umbrales](ruido-y-umbrales.md)— y decidir
qué hacer con los modos secundarios. La variante más robusta ajusta un modelo
gaussiano al modo dominante en vez de sumar líneas crudas, lo que da un
estimador menos sensible a la presencia de un segundo modo, y es la que conviene
como modo por defecto.

Un detalle que muerde: el espectro es circular en velocidad, así que un eco
centrado cerca del borde de Nyquist aparece partido entre los dos extremos del
espectro. Calcular el primer momento sin tener eso en cuenta da una velocidad
absurda cercana a cero. La solución estándar es localizar el máximo, rotar
cíclicamente el espectro para centrarlo, calcular los momentos sobre el espectro
rotado y deshacer la rotación en el resultado.

## Configuraciones cubiertas

Independiente de la polarimetría —se aplica por canal— y del transmisor, con la
salvedad habitual del magnetrón: la corrección de fase debe haberse aplicado
antes, porque sobre una serie con fase aleatoria por pulso el espectro es
plano y no hay nada que estimar.

## Parámetros del contrato que consume

De `config`: `estimator = spectral` lo selecciona, `n_pulses` fija la longitud
de la FFT, `clutter_width_ms` acota la región espectral atribuible a clutter, y
los umbrales de censura actúan igual que con el estimador primario. La
disponibilidad se declara en `estimator_mask` del mensaje `capabilities`; si se
pide y no está compilado, el rechazo es `estimator_unsupported`.

## Criterio de aceptación

El mismo procedimiento de malla (SNR, σv, M) que el pulse-pair, con dos
exigencias añadidas. Primera: en escenarios de un solo modo, el estimador
espectral no debe ser peor que el pulse-pair —si lo es, hay un error en la
ventana o en la atribución de líneas—, y a anchos espectrales grandes debe ser
mejor. Segunda: en escenarios bimodales inyectados con dos poblaciones de
velocidad conocida, debe recuperar el modo dominante con el sesgo declarado,
mientras que el pulse-pair devuelve la media ponderada; ese contraste es la
prueba que justifica la existencia del modo.

## Coste de cómputo

O(M log M) por celda y canal, frente a O(M) del pulse-pair, más la ventana y la
búsqueda del máximo. Con M típicos de 32 a 128 el factor real está entre 5× y
15× el coste del estimador primario, y es la razón por la que se ofrece como
modo alternativo y no como primario. La longitud de FFT debe ser potencia de dos
o soportada eficientemente por `rustfft`; cuando `n_pulses` no lo sea, la
decisión —rellenar con ceros o truncar— cambia el ancho espectral estimado y
tiene que documentarse, no dejarse al azar de la implementación.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulo 6: estimación espectral de momentos y comparación con el pulse-pair.
- Harris, F. J. (1978), «On the Use of Windows for Harmonic Analysis with the Discrete Fourier Transform», *Proceedings of the IEEE* — referencia clásica para la elección de ventana y el compromiso lóbulos laterales/resolución.
- Siggia, A. D. & Passarelli, R. E. (2004), «Gaussian Model Adaptive Processing (GMAP)», *Proceedings of ERAD 2004* — el ajuste gaussiano en el dominio espectral, compartido con el filtro de clutter.
- [LROSE](https://github.com/NCAR/lrose-core) — implementación abierta en C++ del estimador espectral sobre series temporales, con la misma dualidad pulse-pair/espectral.
