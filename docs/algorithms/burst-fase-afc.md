# Burst de transmisión, corrección de fase y AFC

> **Oráculo en Python**: [`tools/oracles/burst_fase_afc.ipynb`](../../tools/oracles/burst_fase_afc.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/burst`: medida de fase/frecuencia del burst, corrección de fase coherent-on-receive y lazo de AFC de primer orden con congelamiento y BITE, contrastada numéricamente contra el oráculo en `crates/burst/tests/against_oracle.rs`. Medida de amplitud como entrada al BITE de potencia, y límites de excursión/velocidad de cambio del lazo, quedan pendientes.

## Qué resuelve

Un estimador Doppler mide el cambio de fase del eco entre pulsos consecutivos.
Eso presupone que la fase de referencia —la del pulso transmitido— es conocida y
estable. Con un transmisor coherente (klistrón, TWT, estado sólido) lo es por
construcción. Con un magnetrón no: el magnetrón es un oscilador libre que
arranca en cada pulso con una fase inicial aleatoria y cuya frecuencia deriva con
la temperatura, el envejecimiento y la tensión de alimentación. Sobre una serie
temporal así, la fase entre pulsos consecutivos es ruido uniforme y la velocidad
estimada no significa absolutamente nada.

La solución clásica es *coherent-on-receive*: no se estabiliza el transmisor, se
mide lo que hizo. Una muestra acoplada del pulso transmitido —el **burst**—
entra por un canal del receptor, y de ella se extraen las dos cantidades que
hacen falta: la fase inicial de ese pulso concreto, que se resta de todos los
ecos de ese pulso, y la frecuencia central del transmisor, que alimenta el lazo
de control automático de frecuencia (AFC) que reajusta el NCO del receptor.

## Cómo funciona

**Medida del burst.** El burst es un tramo corto de muestras I/Q al principio
del rayo, en la ventana temporal en que el pulso transmitido está en el aire. De
ese tramo se estiman dos cosas. La fase inicial se obtiene como el argumento de
la suma coherente de las muestras del burst, ponderada por su amplitud: promediar
antes de tomar el argumento, y no al revés, evita el problema del salto de fase
en ±π. La frecuencia se obtiene de la pendiente de la fase a lo largo del burst
—equivalentemente, del argumento de la autocovarianza a retardo 1 dentro del
burst— o, si el burst es suficientemente largo, de la posición del pico de su
espectro. La amplitud del burst es además la medida de potencia transmitida que
alimenta el BITE.

**Corrección de fase.** Cada muestra de eco del pulso m se multiplica por
`exp(−j·φ_m)`, con φ_m la fase medida del burst de ese mismo pulso. Tras esa
corrección la serie es coherente y todos los estimadores aguas abajo funcionan
sin saber qué tipo de transmisor hay. La etapa es una multiplicación compleja
por muestra, vectorizable de forma trivial, pero toca *todas* las muestras: es la
etapa de mayor volumen de datos del pipeline y merece atención de rendimiento
desde el principio.

**Lazo de AFC.** La frecuencia medida en el burst se compara con la frecuencia
central nominal del receptor. La diferencia se filtra —un lazo de primer o
segundo orden con constante de tiempo de segundos, no de milisegundos, porque la
deriva del magnetrón es lenta y el ruido de la medida por pulso no lo es— y se
convierte en una nueva palabra de fase para el NCO del DRx. El contrato
`DRx↔DSP` transporta esa corrección en el mensaje `Afc` como
**palabra de fase absoluta** (`nco_phase_inc`), no como offset en Hz, decisión
que el proyecto DRx documenta como D-02: en Hz haría falta que el DSP conociera
la frecuencia de muestreo del DRx, y eso acopla las dos plataformas.

El lazo necesita las salvaguardas habituales de cualquier lazo cerrado que
gobierna hardware: límite de excursión máxima respecto de la nominal, límite de
velocidad de cambio por actualización, y detección de pérdida de burst —si el
burst desaparece o su amplitud cae por debajo del umbral, el lazo se congela en
su último valor válido y se emite un evento de BITE, en vez de seguir integrando
ruido hasta desintonizar el receptor.

## Configuraciones cubiertas

**Magnetrón.** Las tres funciones son obligatorias y la corrección de fase es
prerrequisito duro de cualquier estimador Doppler. Esto mueve la etapa a la fase
1 del plan de trabajo, antes que el pulse-pair, y no a la fase 2 donde el plan
del DSP la situó junto al resto de la suite Doppler.

**Klistrón, TWT o estado sólido.** La fase es determinista, así que la
corrección se reduce a compensar un desfase de sistema constante, o se desactiva
por completo. El burst sigue siendo útil como monitor de potencia transmitida y
como referencia de amplitud para la calibración, y el AFC se degrada a un ajuste
lento o desaparece. La etapa se implementa igual y se gobierna por configuración:
el mismo código con la corrección desactivada tiene que producir exactamente la
misma salida que no tener la etapa, y eso es un test.

**Efecto secundario que conviene no perder de vista.** La fase aleatoria del
magnetrón, que aquí es un estorbo, es un recurso para separar ecos de segundo
trip: al corregir la serie con la fase del primer trip, el segundo trip queda
con fase aleatoria y se blanquea en el espectro. Ver
[dealiasing de rango](dealiasing-de-rango.md).

## Parámetros del contrato que consume

Del contrato `DRx↔DSP`: el canal de burst dentro del `channel_mask` del mensaje
`Ray`, y el mensaje `Afc` con `nco_phase_inc` como salida hacia el DRx. Del
contrato `DSP↔RCP`: publica en `status` el `trigger_period_meas_ns` frente al
`trigger_period_cmd_ns` —cuya diferencia es la deriva— y emite eventos
`bite_event` ante pérdida de burst o saturación del lazo.

## Criterio de aceptación

Sobre escenarios simulados de magnetrón con fase aleatoria inyectada y momentos
conocidos, los momentos estimados tras la corrección deben coincidir, dentro del
error estadístico, con los que se obtienen del mismo escenario generado con
transmisor coherente. Ése es el test que importa: la corrección es correcta si y
sólo si borra la diferencia entre los dos tipos de transmisor.

Para el AFC, sobre una deriva de frecuencia inyectada con perfil conocido
—rampa, escalón y ruido—, el lazo debe converger dentro del tiempo declarado, sin
sobreoscilación por encima del margen fijado, y debe congelarse y emitir BITE
ante una pérdida de burst simulada. La medida de frecuencia del burst se
contrasta contra el valor inyectado.

## Coste de cómputo

La medida del burst es despreciable: unas decenas de muestras por rayo. La
corrección de fase es una multiplicación compleja por cada muestra I/Q recibida,
es decir `bins × n_channels × M` operaciones por rayo, la etapa más voluminosa
del pipeline. Candidata directa a SIMD y a fusionarse con la conversión de int16
a punto flotante de la etapa de ingesta, para no recorrer el buffer dos veces.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulo 3: receptores coherent-on-receive y el papel de la muestra de burst.
- Skolnik, M. I. (ed.), *Radar Handbook*, 3ª ed., McGraw-Hill, 2008 — capítulos de transmisores: comportamiento de fase y frecuencia del magnetrón frente a los amplificadores coherentes.
- Zrnić, D. S. & Mahapatra, P. (1985), «Two Methods of Ambiguity Resolution in Pulsed Doppler Weather Radars», *IEEE Transactions on Aerospace and Electronic Systems* — uso de la fase de transmisión, relevante también para el segundo trip.
- [LROSE](https://github.com/NCAR/lrose-core) — su motor de series temporales contempla radares con corrección por burst y documenta el formato en que esa muestra viaja, útil como referencia de arquitectura de la etapa.
