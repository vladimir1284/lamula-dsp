# Recuperación de Segundo Trip (Codificación de Fase Aleatoria / SZ)

## Qué resuelve

El rango no ambiguo `r_max = c·PRT/2` implica que, a PRFs altos (necesarios para buena resolución de velocidad), ecos provenientes de más allá de `r_max` — de un pulso anterior que todavía no completó su viaje de ida y vuelta — pueden llegar superpuestos en el tiempo con ecos del rango actual ("second trip" o "multiple trip echoes"). Sin corrección, esos ecos superpuestos contaminan tanto la reflectividad como la velocidad estimada en la celda de rango ambiguo, produciendo artefactos severos y falsas alarmas en el producto final.

## Cómo funciona la codificación de fase aleatoria (SZ)

El esquema **SZ(8/64)**, introducido por Sachidananda & Zrnić (1999) en "Systematic Phase Codes for Resolving Range Overlaid Signals in a Doppler Weather Radar" (*Journal of Atmospheric and Oceanic Technology*), resuelve el problema modulando sistemáticamente la fase de los pulsos transmitidos según un código conocido (un patrón de 64 pulsos derivado de una secuencia de 8 fases base, de ahí "8/64"). Como el eco del "primer trip" (rango actual) y el del "segundo trip" (rango superpuesto) recorren distinto número de intervalos de PRT antes de llegar, la modulación de fase sistemática los desplaza de forma *distinta* en el dominio de la frecuencia Doppler tras aplicar el código de decodificación correspondiente a cada trip. El resultado es que, al decodificar la serie temporal asumiendo que la señal proviene del primer trip, la componente del primer trip queda coherente (un pico espectral nítido) mientras la del segundo trip queda "blanqueada" — dispersada en una banda ancha y aproximadamente plana de baja densidad espectral de potencia. Esto permite separar ambas contribuciones en el dominio espectral y estimar los momentos de cada trip de forma independiente, incluso cuando comparten la misma celda temporal.

Esta técnica es sustancialmente más sofisticada que el enfoque clásico previo (PRF staggering para desambiguación de rango, o simplemente descartar/marcar como contaminadas las celdas de segundo trip), y es la razón por la que los procesadores de gama alta pueden ofrecer rango completo y buena resolución de velocidad simultáneamente sin las zonas ciegas de rango ambiguo típicas de esquemas más simples.

## Relevancia para LAMULA DSP

Dado que esta técnica exige generar en transmisión un patrón de fase específico (co-diseño con el excitador/DRx) y decodificar en el DSP con el algoritmo espectral correspondiente, LAMULA DSP la documenta como capacidad de **Stage 2 / diferida** salvo que el hardware de excitación ya soporte modulación de fase programable pulso a pulso — se registra aquí como referencia de diseño para cuando ese workstream se aborde, en vez de comprometerse a M4 del Stage 1.

## Configuraciones cubiertas

Ésta es la página del conjunto con la dependencia de hardware más estricta.
SZ(8/64) exige un excitador capaz de imponer una fase programada pulso a pulso,
lo que en la práctica significa transmisor coherente con modulación de fase
gobernable. **Confirmado (2026-09-04): con klistrón, el excitador de la
instalación sí soporta esta modulación** — el bloqueo de hardware que motivaba
la deferencia a Stage 2 queda cerrado para esta variante del Eje 1; lo que
sigue abierto es sólo de alcance/prioridad de roadmap, no de capacidad física.
**Con un magnetrón no se puede aplicar SZ** —la fase no se elige— pero
sí existe la vía análoga que explota la aleatoriedad natural del magnetrón, que
se describe en [dealiasing de rango](dealiasing-de-rango.md). Es decir: las dos
configuraciones tienen camino, pero son caminos distintos, y ninguno de los dos
es «SZ con otro nombre».

La polarimetría es ortogonal, con la nota de que en modo alternante hay la mitad
de muestras por canal y la separación estadística de los trips empeora en
consecuencia.

## Parámetros del contrato que consume

De `config`: `range_dealias` como interruptor. La capacidad real se declara en
`capability_flags` del mensaje `capabilities`, y una instalación que no pueda
codificar fase debe declararlo así en vez de aceptar el bit sin hacer nada.
Publica `unambiguous_range_m`, que el contrato define como `c/(2·PRF)` salvo
recuperación de trip.

## Oráculo

[`tools/oracles/sz_second_trip_recovery.ipynb`](../../tools/oracles/sz_second_trip_recovery.ipynb) —
paso 1 del método, hecho pese a la deferencia a Stage 2 (ver "Relevancia para
LAMULA DSP" arriba), a pedido explícito. Cubre construcción del código
(verificación de periodicidad a 32 pulsos y de la estructura de 8 réplicas
espectrales equiespaciadas), sesgo de velocidad del trip fuerte con y sin
código, y recuperación de velocidad del trip débil vía notch + recoherencia,
en función de la razón de potencias entre trips. La fórmula del código se tomó
de una fuente secundaria (Meymaris, Hubbert & Ellis 2005, AMS — de acceso
público) que cita literalmente a Sachidananda & Zrnić (1999); el paper original
está bloqueado en este entorno (mismo 403 de AMS/ResearchGate ya documentado en
`roadmap.md` para la varianza de pulse-pair). Fuera de alcance, declarado en el
propio notebook: ancho espectral de cualquiera de los dos trips (exige
"magnitude deconvolution", no implementada), potencia del trip fuerte vía
notch (exige un factor de corrección según ancho de notch, no reproducido), y
más de dos trips solapados. La confirmación de hardware (excitador con fase
programable) que faltaba al escribir este oráculo ya llegó — ver más abajo.

## Implementación

`crates/sz864` (paso 2 y 3 del método): `sz_8_64_phases` (construcción del
código, con los mismos dos tests de la Prueba 1 del oráculo como tests
unitarios deterministas — periodicidad a 32 pulsos y las 8 réplicas
equiespaciadas) y `separate_trips` (decodificación al trip fuerte, notch
centrado en su velocidad ya estimada, recoherencia del residuo al trip
débil), contrastado numéricamente contra el oráculo en
`crates/sz864/tests/against_oracle.rs` (Pruebas 2 y 3, mismas tolerancias).
Reutiliza sin reimplementar `lamula_burst::correct_phase`,
`lamula_moments::pulse_pair_moments`, `lamula_dual_prf::fold` y
`lamula_spectral::bin_velocity`. `cargo build`/`cargo test --workspace`/
`cargo clippy -- -D warnings`/`cargo fmt --check` en verde. Mismo alcance y
mismas exclusiones que el oráculo (ver arriba): sólo velocidad está
contrastada para el trip débil, no potencia ni ancho espectral.

**Sin cablear en `crates/service::ray`.** Este crate es librería pura, sin
sitio de llamada en el pipeline — cablearlo exige antes decidir cómo el
contrato declara "instalación klistrón con código SZ activo" (algo análogo a
`polarization_mode`/`antenna_isolation_db`: un campo nuevo, no una inferencia
de `capability_flags::range_dealias` existente, que hoy no distingue esta
vía de la de fase aleatoria en magnetrón) y cómo el excitador recibe el
patrón de fase pulso a pulso a transmitir — trabajo de contrato e
integración con DRx, no de algoritmo.

## Criterio de aceptación

Cuando este trabajo se aborde: escenarios con eco de segundo trip inyectado a
rango y momentos conocidos sobre eco de primer trip también conocido, y el
criterio expresado como curva de exactitud de los momentos de cada trip frente a
la razón de potencias entre ambos. La cifra que caracteriza la implementación es
la supresión alcanzada en dB, comparable con la publicada en la literatura para
el mismo código.

## Coste de cómputo

Procesamiento espectral por celda y por trip, con la decodificación de fase
correspondiente a cada trip: del orden de una FFT por trip además del filtrado de
clutter, que en presencia de trips superpuestos también hay que rehacer por trip.
Es el algoritmo más caro del conjunto y su viabilidad en el hardware objetivo
tiene que medirse antes de comprometerlo.

## Referencias abiertas / implementaciones libres

- Sachidananda, M. & Zrnić, D. S. (1999), "Systematic Phase Codes for Resolving Range Overlaid Signals in a Doppler Weather Radar", *Journal of Atmospheric and Oceanic Technology*, vol. 16.
- Sachidananda, M. & Zrnić, D. S. (2000), "Clutter Filtering and Spectral Moment Estimation for Doppler Weather Radars Using Staggered Pulse Repetition Time (PRT)" — trabajo complementario sobre PRT escalonado como alternativa/complemento.
- [LROSE / RadX](https://github.com/NCAR/lrose-core) — el motor de procesamiento de NCAR incluye utilidades relacionadas con PRT escalonado y desambiguación de rango, útiles como referencia de arquitectura de decodificación aunque no implementen SZ(8/64) exactamente.
