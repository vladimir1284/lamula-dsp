# Variables polarimétricas: ZDR, ρHV, ΦDP y LDR

> **Oráculo en Python**: [`tools/oracles/polarimetria_covarianzas.ipynb`](../../tools/oracles/polarimetria_covarianzas.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/polarimetry`: ZDR/ρHV/ΦDP en modo simultáneo, ρHV corregido por decorrelación de retardo medio-PRT en modo alternante (Sachidananda & Zrnić 1989) y LDR con saturación por aislamiento de antena, contrastada numéricamente contra el oráculo en `crates/polarimetry/tests/against_oracle.rs`. La corrección de ΦDP en modo alternante por el término de fase Doppler del retardo medio-PRT queda pendiente — el oráculo no la valida todavía.

## Qué resuelve

Un radar de canal único mide cuánta potencia devuelve un volumen y a qué
velocidad se mueve. Un radar polarimétrico mide además cómo son los dispersores:
si son achatados —gotas grandes de lluvia—, esféricos —llovizna, granizo en
caída caótica—, alargados y alineados —cristales de hielo—, o si en el mismo
volumen hay una mezcla de cosas distintas. Eso es lo que permite distinguir
lluvia de granizo, detectar la banda brillante de fusión, identificar ecos no
meteorológicos —pájaros, insectos, clutter— y mejorar sustancialmente la
estimación de tasa de precipitación.

Las cuatro variables que el contrato publica salen todas de la misma fuente: la
matriz de covarianza entre los canales de recepción. Por eso van en una sola
página; separarlas induce a implementarlas como cuatro cálculos independientes,
que es la forma cara y propensa a inconsistencias de hacerlo.

## Cómo funciona

Se estiman, por celda de rango y sobre los M pulsos del radial, las potencias de
cada canal y la covarianza cruzada entre canales, todas con su ruido restado:

- **ZDR (reflectividad diferencial)**, en dB, es la razón de potencias copolares:
  `ZDR = 10·log10(P_h / P_v) − offset`. Positivo para gotas achatadas, cercano a
  cero para dispersores esféricos. Es la variable más sensible a errores de
  calibración relativa entre canales, y de ahí que el contrato le dedique un
  campo propio, `zdr_offset_db`.
- **ρHV (coeficiente de correlación copolar)**, adimensional, es el módulo
  normalizado de la covarianza cruzada: `ρHV = |R_hv| / sqrt(P_h · P_v)`. Vale
  cerca de 1 en precipitación homogénea y cae cuando el volumen contiene una
  mezcla de tipos de dispersor o cuando el eco no es meteorológico. Es el mejor
  discriminante de clutter y de eco biológico que existe, y también el más
  sensible a una resta de ruido mal hecha: sin restar el ruido de cada canal,
  ρHV cae artificialmente a SNR baja y todo el producto de clasificación se
  desplaza.
- **ΦDP (fase diferencial)**, en grados, es el argumento de esa misma covarianza
  cruzada, menos la fase diferencial del sistema (`phidp_offset_deg`). Crece
  monótonamente a lo largo del camino de propagación a través de lluvia, y esa
  monotonía es la base del [KDP](kdp-estimacion.md).
- **LDR (razón de despolarización lineal)**, en dB, es la razón entre la potencia
  recibida en polarización cruzada y la copolar cuando se transmite en una sola
  polarización: `LDR = 10·log10(P_vh / P_hh)`. Su rango útil está acotado por el
  aislamiento de polarización cruzada de la antena: si la antena da 30 dB de
  aislamiento, LDR por debajo de −27 dB aproximadamente no es una medida del
  meteoro sino de la antena, y publicarlo como si lo fuera es engañoso.

## Configuraciones cubiertas

Éste es el algoritmo donde la configuración de hardware más cambia las cosas, y
el DSP tiene que soportar los tres casos declarándolos por capacidades.

**Canal único.** No hay ninguna variable polarimétrica. `moment_mask` de
`capabilities` no las incluye y cualquier petición se rechaza con
`moment_unsupported`. Todo lo anterior de esta página queda inactivo, no
degradado.

**Simultánea (STAR / SHV).** Se transmite en polarización a 45° —o se transmite H
y V a la vez— y se reciben los dos canales copolares al mismo tiempo. Se obtienen
ZDR, ρHV y ΦDP con las expresiones directas de arriba, sobre muestras
simultáneas, sin pérdida de PRF. **No se obtiene LDR**, porque nunca se recibe un
canal cruzado con una transmisión de polarización única. El precio es el
acoplamiento cruzado: la señal que retorna en la polarización contraria se suma
a la copolar y sesga ZDR y ΦDP, con un sesgo que depende de la fase diferencial
del propio meteoro y que por tanto no se corrige con un offset constante. Es un
límite conocido del modo, no un defecto de implementación, y se documenta como
tal.

**Alternante (H/V conmutados pulso a pulso).** Se transmite alternando la
polarización y se reciben copolar y cruzada. Hay LDR y no hay acoplamiento
cruzado, pero la PRF efectiva por canal es la mitad, y con ella la velocidad de
Nyquist. Además, las muestras de los dos canales ya no son simultáneas: la
covarianza cruzada se estima a retardo medio-PRT en vez de a retardo cero, lo
que introduce un factor de decorrelación que **hay que corregir** —depende del
ancho espectral— o ρHV sale sistemáticamente bajo y ΦDP sesgado. Sachidananda &
Zrnić (1989) dan el estimador correcto para este modo, y es un estimador
distinto, no el mismo con otros índices. Implementarlo como si fuera el mismo es
el error clásico de este modo.

La consecuencia de diseño es que la etapa de polarimetría es una etapa con tres
implementaciones detrás de una interfaz, seleccionada por el modo declarado del
hardware, y que el conjunto de momentos publicable cambia con ella.

## Parámetros del contrato que consume

De `config`: `moment_mask` para saber qué se pide, `zdr_offset_db` y
`phidp_offset_deg` como correcciones de calibración, `n_pulses` como número de
muestras, y los suelos de ruido por canal del bloque de `status` para la resta
por canal. Del contrato `DRx↔DSP`: `n_channels` y `channel_mask` del mensaje
`Ray` determinan qué modo es físicamente posible. Publica los momentos `zdr`,
`phidp`, `ldr` y `rhohv`.

## Criterio de aceptación

Sobre pares de series simuladas con matriz de covarianza prescrita —el
[simulador](simulador-iq.md) las genera con ZDR, ρHV y ΦDP conocidos— los
estimadores deben recuperar los tres valores sin sesgo, con una desviación
estándar acorde a M y al ρHV verdadero. Dos pruebas específicas por su capacidad
de detectar el error típico: primero, un barrido de SNR descendente en el que
ρHV debe mantenerse plano tras la resta de ruido —si cae, la resta por canal está
mal—; segundo, en modo alternante, la comparación del estimador corregido contra
el estimador simultáneo aplicado ingenuamente a los mismos datos, que debe
mostrar la diferencia esperada por decorrelación y no coincidir. Para LDR, la
verificación de que el valor se satura en el nivel de aislamiento de antena
configurado y se marca como no fiable por debajo de él.

## Coste de cómputo

Del orden de tres acumuladores complejos por celda —`P_h`, `P_v`, `R_hv`— sobre
M pulsos: es del mismo orden que el pulse-pair y se calcula en la misma pasada
sobre los datos, que es como debe implementarse. La memoria y el ancho de banda,
en cambio, se duplican respecto al canal único, y eso sí se nota en el
presupuesto de tiempo real.

## Referencias abiertas / implementaciones libres

- Bringi, V. N. & Chandrasekar, V., *Polarimetric Doppler Weather Radar: Principles and Applications*, Cambridge University Press, 2001 — referencia canónica; definición y estimación de las cuatro variables.
- Sachidananda, M. & Zrnić, D. S. (1989), «Efficient Processing of Alternately Polarized Radar Signals», *Journal of Atmospheric and Oceanic Technology* — el estimador específico del modo alternante.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulo 8: fundamentos de polarimetría.
- [Py-ART](https://github.com/ARM-DOE/pyart) — trabaja con estas variables ya estimadas y aporta las utilidades de clasificación que las consumen; sirve para validar rangos de valores por tipo de eco.
- [LROSE](https://github.com/NCAR/lrose-core) — implementación abierta en C++ de la estimación de covarianzas polarimétricas a partir de series temporales, incluyendo modo alternante; el oráculo más directo de esta página.
