# Ruido, resta de ruido y umbrales de censura

## Qué resuelve

Toda celda de rango contiene, además del eco, ruido térmico del receptor. A SNR
alta es irrelevante; a SNR baja domina, y si no se resta explícitamente sesga la
reflectividad hacia arriba —una celda vacía reporta la potencia del ruido como
si fuera señal— y sesga el ancho espectral, porque el ruido es blanco y ensancha
el espectro aparente. La consecuencia operativa es un radar que «ve» lluvia
donde no la hay en los bordes del alcance, que es el artefacto más visible que
puede tener un producto.

El problema tiene dos mitades: conocer la potencia de ruido con precisión
suficiente, y decidir qué celdas tienen señal bastante para publicarse y cuáles
se censuran.

## Cómo funciona

**Estimación del suelo de ruido.** Hay tres vías, y un procesador serio usa las
tres en distintos momentos. La primera es la medida directa en un intervalo
pasivo: celdas de rango más allá del alcance útil, o un dwell con el transmisor
inhibido, donde por construcción sólo hay ruido; es la más limpia y la que
alimenta el valor de referencia del arranque. La segunda es el método objetivo
de Hildebrand & Sekhon (1974), que separa la parte de ruido del espectro Doppler
sin necesidad de saber a priori dónde está: ordena las líneas espectrales por
potencia y busca el umbral a partir del cual el conjunto de líneas por debajo se
comporta como ruido blanco, comprobando la relación esperada entre media y
varianza de un espectro de ruido. La tercera, para operación continua, es la
estimación por radial de Ivić, Curtis & Torres (2013), pensada para seguir la
deriva del receptor sin interrumpir el escaneo.

**Resta.** Una vez conocida la potencia de ruido N, la potencia de señal es
`S = R(0) − N`, con R(0) la potencia total medida. La resta se hace en lineal, no
en dB, y el resultado se recorta a cero: por debajo del suelo, la estimación de
una realización finita da valores negativos con probabilidad apreciable, y
propagarlos a un logaritmo produce NaN o infinitos. Esa celda no es «potencia
negativa», es «sin señal detectable», y se marca como tal.

**Umbrales.** El contrato expone cuatro y cada uno censura por un motivo
distinto, que es la razón de que no se colapsen en uno. `sig_threshold` censura
por SNR insuficiente. `log_threshold` censura por potencia logarítmica absoluta,
independientemente del ruido. `sqi_threshold` censura por coherencia
insuficiente de la serie temporal, que detecta el caso en que hay potencia pero
no es un eco meteorológico coherente. `ccor_threshold` censura por corrección de
clutter excesiva, es decir, celdas donde lo que quedó tras el filtro es tan poco
frente a lo que se quitó que el residuo no es fiable. Una celda censurada se
codifica como NaN en el bloque de momento —el contrato lo prevé con
`moment_flag.has_missing`— y el radial se marca con `ray_flag.censored`.

La decisión de diseño que conviene fijar explícitamente: **se censura el momento
publicado, no la muestra de entrada**, y los cuatro umbrales se evalúan sobre
cantidades ya calculadas. Censurar antes de estimar impide reportar el índice de
calidad que explicaría por qué la celda se descartó.

## Configuraciones cubiertas

Es independiente del tipo de transmisor y de la polarimetría, con dos matices.
Con dos canales, cada canal tiene su propio suelo de ruido —el contrato dedica
`noise_floor_dbm_0` a `noise_floor_dbm_3` a eso— y la resta se hace por canal
antes de calcular cualquier variable polarimétrica; usar un único valor
promediado sesga ZDR de forma proporcional a la diferencia entre canales, que es
justamente lo que el ZDR mide. Con magnetrón, la deriva de frecuencia mueve la
señal dentro de la banda del filtro receptor, así que la estimación del suelo de
ruido debe repetirse tras cualquier corrección de AFC significativa.

## Parámetros del contrato que consume

De `config`: `noise_floor_dbm`, `sig_threshold`, `log_threshold`,
`sqi_threshold`, `ccor_threshold`. Publica en `status` el suelo de ruido vigente
por canal (`noise_floor_dbm_0..3`) y el offset de continua por canal, y en cada
radial el `noise_floor_dbm` con el que se procesó.

## Criterio de aceptación

Sobre escenarios simulados con potencia de señal conocida y ruido conocido, la
reflectividad estimada debe seguir a la verdad-terreno sin sesgo apreciable
hasta SNR ≈ 0 dB, y el sesgo residual por debajo de ese punto debe quedar
acotado y documentado. Sobre celdas puramente de ruido, la tasa de falsos
positivos —celdas que superan los umbrales sin haber señal— debe quedar por
debajo del valor declarado para cada configuración de umbrales. El estimador de
Hildebrand & Sekhon se contrasta contra el valor inyectado en el simulador, que
es conocido exactamente.

## Coste de cómputo

La resta y los umbrales son O(1) por celda y despreciables. La estimación
objetiva del suelo de ruido requiere ordenar las líneas espectrales, O(M log M)
por celda, pero no hace falta hacerla en cada radial: es una cantidad de
evolución lenta y basta refrescarla cada cierto número de radiales o por dwell
dedicado.

## Referencias abiertas / implementaciones libres

- Hildebrand, P. H. & Sekhon, R. S. (1974), «Objective Determination of the Noise Level in Doppler Spectra», *Journal of Applied Meteorology*.
- Ivić, I. R., Curtis, C. & Torres, S. M. (2013), «Radial-Based Noise Power Estimation for Weather Radars», *Journal of Atmospheric and Oceanic Technology*.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulo 6: efecto del ruido sobre los estimadores de momentos.
- [Py-ART](https://github.com/ARM-DOE/pyart) — `pyart.retrieve` y sus utilidades de SNR y de censura por umbral, útiles como contraste del criterio de censura.
- [LROSE](https://github.com/NCAR/lrose-core) — implementación abierta en C++ de la estimación de ruido por el método objetivo, aplicable como oráculo directo.
