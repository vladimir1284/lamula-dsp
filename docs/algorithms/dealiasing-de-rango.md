# Dealiasing de rango: ecos de trip múltiple

> **Oráculo en Python**: [`tools/oracles/dealiasing_de_rango.ipynb`](../../tools/oracles/dealiasing_de_rango.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/range-dealias`: detección/marcado por reconciliación dual-PRF y recuperación de primer trip por fase aleatoria en magnetrón, reutilizando sin reimplementar `crates/burst` y `crates/moments`, contrastada numéricamente contra el oráculo en `crates/range-dealias/tests/against_oracle.rs`. SZ(8/64) sigue diferido a Stage 2 en su propia página.

## Qué resuelve

A PRF alta, los ecos que llegan de más allá del rango no ambiguo `r_max = c·PRT/2`
se solapan en el tiempo con los del trip actual: una celda de rango contiene la
suma de un eco cercano y otro que en realidad está a `r + r_max`. Sin tratamiento,
esa contaminación aparece como reflectividad y velocidad falsas, típicamente como
manchas de eco «fantasma» que se mueven al cambiar la PRF, y es uno de los
artefactos que más desconfianza generan en un operador.

El contrato ofrece `range_dealias` como interruptor en v0.1, y la página de
[SZ(8/64)](sz-second-trip-recovery.md) difiere a Stage 2 la técnica más
sofisticada. Esta página resuelve qué hay detrás de ese interruptor en Stage 1, y
la respuesta depende del hardware.

## Cómo funciona

Las técnicas disponibles forman una escala de capacidad, y cada instalación
alcanza un peldaño según lo que su transmisor permita.

**Detección y marcado (siempre disponible).** Sin ninguna capacidad especial se
puede detectar la sospecha de solapamiento y marcarla en vez de corregirla. La
vía práctica es comparar el mismo azimut medido con dos PRFs distintas —lo que
los modos de corte ya proporcionan—: un eco de primer trip aparece en la misma
posición con las dos, uno de trip superior se desplaza. Complementariamente, una
celda cuya potencia es incoherente con la de sus vecinas en rango y cuyo perfil
no corresponde a nada meteorológico es candidata. La celda se censura o se marca,
y el contrato ya tiene el vocabulario: `ray_flag.censored` y NaN en el bloque de
momento. Es poco, pero es honesto, y es infinitamente mejor que publicar el
número contaminado sin decirlo.

**Recuperación por fase aleatoria (magnetrón).** Aquí la característica que en
[la corrección de fase](burst-fase-afc.md) era un estorbo se vuelve un recurso.
Cuando la serie se corrige con la fase del burst correspondiente al *primer*
trip, la componente del primer trip queda coherente y la del segundo trip
—que fue transmitida en un pulso anterior, con otra fase aleatoria— queda con
fase aleatoria residual y se blanquea: se dispersa por todo el espectro Doppler
como un pedestal de baja densidad espectral en vez de concentrarse en un pico.
Eso permite estimar los momentos del primer trip filtrando ese pedestal, y
—recorriendo la serie con la secuencia de fases del segundo trip— recuperar
también los del segundo. Es el mismo principio que explota SZ, pero con la
aleatoriedad que el magnetrón regala en vez de una codificación diseñada. La
contrapartida es que una secuencia aleatoria no da la separación limpia y
predecible de un código sistemático, y la supresión alcanzable es menor.

**Recuperación por código sistemático (transmisor coherente con modulación de
fase programable).** Es SZ(8/64), documentado en su
[propia página](sz-second-trip-recovery.md) y diferido a Stage 2. Requiere que el
excitador acepte una fase programada pulso a pulso.

**Vía indirecta: PRT escalonado.** El [staggered-PRT](staggered-prt.md), pensado
para la ambigüedad de velocidad, aporta de paso información de rango: un eco de
trip superior aparece en posiciones distintas según el periodo del pulso que lo
originó, lo que permite identificarlo aunque no siempre separarlo.

## Configuraciones cubiertas

Lo anterior es literalmente el eje de variabilidad del transmisor aplicado a este
problema. La recomendación de diseño es que Stage 1 implemente la detección y
marcado —que vale para todas las instalaciones— y la recuperación por fase
aleatoria cuando el hardware sea de magnetrón, y que el bit `range_dealias` se
declare en `capability_flags` según lo que la instalación concreta soporte, con
rechazo limpio en el resto de casos. Lo que no debe hacerse es aceptar el bit y
no hacer nada: el RCP creería que el problema está tratado.

La polarimetría es ortogonal a esto, con una nota: en modo alternante hay la
mitad de muestras por canal, lo que empeora la separación estadística de los dos
trips y hay que tenerlo en cuenta en el criterio de aceptación.

## Parámetros del contrato que consume

De `config`: `range_dealias` como interruptor, `prf_hz` —que fija `r_max` y por
tanto la posición del solapamiento— y los umbrales de censura. Publica
`unambiguous_range_m` por radial, que el contrato documenta explícitamente como
`c/(2·PRF)` **salvo recuperación de trip**: cuando la recuperación está activa y
funciona, ese campo es el sitio donde se declara el alcance realmente
desambiguado.

## Criterio de aceptación

El [simulador](simulador-iq.md) genera escenarios con eco de segundo trip
inyectado a rango y momentos conocidos, superpuesto a un eco de primer trip
también conocido. Con la recuperación desactivada, el sistema debe **detectar y
marcar** las celdas contaminadas, medido como tasa de detección y tasa de falsa
alarma. Con la recuperación activa en configuración de magnetrón, los momentos
del primer trip deben recuperarse dentro del margen declarado en función de la
razón de potencias entre los dos trips —el criterio tiene que ser una curva
frente a esa razón, no un número único, porque la técnica degrada suavemente y
cualquier valor único oculta dónde deja de funcionar.

## Coste de cómputo

La detección por comparación entre PRFs es aritmética por celda, despreciable. La
recuperación por fase aleatoria requiere procesamiento espectral —una FFT por
celda y trip— y por tanto tiene el mismo orden de coste que el
[estimador espectral](estimador-espectral.md), multiplicado por el número de
trips que se quieran recuperar.

## Referencias abiertas / implementaciones libres

- Sachidananda, M. & Zrnić, D. S. (1999), «Systematic Phase Codes for Resolving Range Overlaid Signals in a Doppler Weather Radar», *Journal of Atmospheric and Oceanic Technology* — la técnica de referencia, y el marco teórico que también explica el caso de fase aleatoria.
- Zrnić, D. S. & Mahapatra, P. (1985), «Two Methods of Ambiguity Resolution in Pulsed Doppler Weather Radars», *IEEE Transactions on Aerospace and Electronic Systems* — tratamiento clásico de la ambigüedad de rango, incluida la explotación de la fase de transmisión.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulo 7: ambigüedades de rango y velocidad, y el compromiso entre ambas.
- [LROSE](https://github.com/NCAR/lrose-core) — utilidades de procesamiento con PRT escalonado y desambiguación de rango; referencia de arquitectura de la etapa de decodificación.
