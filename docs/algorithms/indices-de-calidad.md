# Índices de calidad: SQI, CCOR y SIG

> **Oráculo en Python**: [`tools/oracles/indices_de_calidad.ipynb`](../../tools/oracles/indices_de_calidad.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/quality`: SQI, CCOR y SIG como funciones puras sobre cantidades ya calculadas por otros algoritmos, contrastada numéricamente contra el oráculo en `crates/quality/tests/against_oracle.rs`.

## Qué resuelve

Un momento sin índice de calidad es un número sin contexto: el consumidor no
puede distinguir una velocidad radial estimada sobre un eco fuerte y coherente
de otra estimada sobre ruido. Los tres índices que el contrato publica como
momentos de pleno derecho responden a tres preguntas distintas —¿es coherente la
serie temporal?, ¿cuánto tuvo que quitar el filtro de clutter?, ¿cuánta señal hay
sobre el ruido?— y por eso no se pueden reducir a un único «indicador de
calidad».

## Cómo funciona

**SQI (Signal Quality Index).** Es el módulo de la autocorrelación normalizada a
retardo 1: `SQI = |R(1)| / R(0)`, con R(0) la potencia total y R(1) la
autocovarianza a retardo 1 que el [pulse-pair](pulse-pair-moments.md) ya calcula.
Vale 1 para una señal perfectamente coherente —un tono puro, típicamente clutter
o un blanco puntual— y tiende a 0 para ruido blanco. Un eco meteorológico normal
cae en medio, y su valor está ligado al ancho espectral: cuanto más ancho el
espectro, menor la correlación a retardo 1. Su utilidad práctica es censurar
celdas donde la velocidad estimada no es fiable, porque la varianza del
estimador de velocidad crece justamente cuando SQI cae.

**CCOR (Clutter Correction).** Es la razón, en dB, entre la potencia que queda
tras el filtro de clutter y la que había antes: `CCOR = 10·log10(P_filtrada /
P_total)`. Es negativa o cero por construcción, y su magnitud dice cuánta
potencia se quitó. Un CCOR muy negativo señala una celda dominada por clutter en
la que lo que sobrevivió al filtro es un residuo pequeño de una cantidad grande,
con el error relativo que eso implica; de ahí que sea un criterio de censura y
no sólo un dato informativo. Se calcula siempre que el filtro esté activo, y
vale 0 dB cuando no lo está.

**SIG (Signal-to-Noise Ratio).** Es la relación señal-ruido en dB tras la resta
de ruido: `SIG = 10·log10((R(0) − N) / N)`. Comparte con la
[cadena de ruido](ruido-y-umbrales.md) tanto el valor de N como el recorte a
cero de la señal residual.

La relación entre los tres importa para el orden del pipeline: CCOR sólo existe
después del filtro de clutter, SQI se calcula sobre la serie que efectivamente
se usó para estimar la velocidad —es decir, la filtrada, si hay filtro— y SIG se
calcula sobre la potencia total, filtrada o no según lo que se esté publicando.
Documentar esta elección importa más que la elección misma: un SQI calculado
antes del filtro y otro calculado después son cantidades distintas, y comparar
productos entre procesadores que hicieron elecciones opuestas produce
discrepancias que luego cuesta semanas rastrear.

## Configuraciones cubiertas

Los tres son independientes del tipo de transmisor y existen también en radar de
canal único. En configuración polarimétrica se calculan sobre el canal copolar
horizontal por convenio; con recepción alternante, sobre la serie del canal que
corresponda al momento que se está censurando.

## Parámetros del contrato que consume

Publica los momentos `sqi`, `ccor` y `sig` del enum `moment_kind`. Consume de
`config` los umbrales homónimos `sqi_threshold`, `sig_threshold` y
`ccor_threshold` para la censura descrita en
[ruido y umbrales](ruido-y-umbrales.md).

## Criterio de aceptación

Sobre señal simulada con ancho espectral conocido, el SQI medido debe seguir la
relación analítica entre correlación a retardo 1 y ancho espectral del modelo
gaussiano, dentro del error estadístico correspondiente a M muestras. Sobre
ruido puro debe converger a 0 con la dispersión esperada, y sobre un tono puro a
1. El CCOR debe reproducir exactamente, dentro de la precisión numérica, la
razón de potencias antes y después del filtro en escenarios con clutter
inyectado de potencia conocida. El SIG se valida junto con la resta de ruido.

## Contraste cruzado contra SIGMET RVP8

Ver [pulse-pair](pulse-pair-moments.md) §"Contraste cruzado contra SIGMET
RVP8" para el detalle: SQI coincide de forma literal con la definición del
*RVP8 User's Manual* (`|R(1)|/R(0)`); CCOR coincide en su forma general; SIG
persigue la misma cantidad física (SNR tras clutter) por un camino algebraico
distinto — el manual la arma sobre `R0`/`T0` sin filtrar, este repo la calcula
directamente sobre la señal ya filtrada de clutter y ruido. Ninguno de los
tres reemplaza el contraste de varianza teórica que pide el "Criterio de
aceptación" de arriba, que sigue sin cerrar por falta de acceso al capítulo 6
de Doviak & Zrnić en este entorno.

## Coste de cómputo

Despreciable: los tres se derivan de cantidades que el estimador de momentos ya
calculó. No justifican ninguna optimización específica.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulo 6: relación entre la correlación a retardo 1, el ancho espectral y la varianza de los estimadores.
- [LROSE](https://github.com/NCAR/lrose-core) — calcula índices equivalentes (NCP, *normalized coherent power*, que es el mismo cociente que SQI) sobre series temporales reales; sirve de oráculo directo.
- [Py-ART](https://github.com/ARM-DOE/pyart) — expone NCP en sus lectores de datos de radares que lo publican, útil para contrastar rangos de valores esperables por tipo de eco.
