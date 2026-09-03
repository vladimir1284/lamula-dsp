# Corrección de atenuación por lluvia (Z-PHI)

> **Oráculo en Python**: [`tools/oracles/atenuacion_zphi.ipynb`](../../tools/oracles/atenuacion_zphi.ipynb) — derivado de la reconstrucción de la fórmula descrita en esta página (ver "Cómo funciona" para el alcance exacto de esa reconstrucción), no de ningún código Rust (`roadmap.md` §"Método de estudio"). Implementación Rust en `crates/attenuation`: perfil de atenuación específica cerrado (Testud et al. 2000) y reflectividad corregida, contrastados numéricamente contra el oráculo en `crates/attenuation/tests/against_oracle.rs`. Cableado en el campo CZ de `crates/service::ray` sobre cada tramo contiguo con segundo canal válido.

## Qué resuelve

La reflectividad medida por un radar meteorológico en banda C o X se atenúa al atravesar lluvia intensa: el propio meteoro entre el radar y una celda lejana absorbe y dispersa energía en los dos sentidos del viaje, así que la Z medida en esa celda es sistemáticamente más baja que la verdadera — y el error crece con el rango recorrido a través de la lluvia, no es un offset fijo corregible con la calibración del sistema. Sin corregirlo, un núcleo convectivo intenso puede "sombrear" el eco de lo que hay detrás, subestimando reflectividad y, con ella, la tasa de lluvia derivada.

La observación de Bringi, Chandrasekar y, sobre todo, Testud et al. (2000) es que un radar polarimétrico ya mide, en la fase diferencial ΦDP, una cantidad que es **inmune a la propia atenuación** (`docs/algorithms/kdp-estimacion.md` §"Qué resuelve") y que está físicamente acoplada a ella: donde hay más agua líquida en el camino hay más atenuación Y más avance de fase diferencial. Eso permite usar el ΔΦDP medido en un tramo como restricción para resolver, de forma cerrada y sin necesitar la constante de calibración de la relación Z-atenuación (que varía mucho con la distribución de tamaño de gota), el perfil de atenuación específica a lo largo de ese tramo.

## Cómo funciona

Sobre un tramo contiguo de lluvia `r1..r2` con reflectividad ya calibrada y filtrada de clutter (CZ, `reflectivity-calibration.md`) y con la fase diferencial total medida en los dos extremos (`ΔΦDP = ΦDP(r2) - ΦDP(r1)`, ya desdoblada — `kdp-estimacion.md` §"Desdoblado de ΦDP"), el método resuelve el perfil de atenuación específica `A(r)` [dB/km] de forma cerrada:

```
I(r1, r) = 0.46·β·∫[r1..r] Z(s)^β ds        (integral prefijo, regla del trapecio)
c        = 10^(0.1·β·a_coef·ΔΦDP) - 1
A(r)     = Z(r)^β · c / (I(r1,r2) + c·(I(r1,r2) - I(r1,r)))
```

con `β` el exponente de la relación Z-atenuación (`A_zA = α·Z^β`) y `a_coef` [dB/grado] el coeficiente de acoplamiento entre atenuación específica y KDP (`A ≈ a_coef·KDP`). La elegancia del método —y la razón de que `α` no aparezca en ninguna parte de la fórmula— es que la constante de la relación Z-atenuación se cancela algebraicamente al imponer la restricción de ΔΦDP; sólo hace falta `β` (que gobierna cómo se **reparte** la atenuación a lo largo del tramo según la forma del perfil de Z) y `a_coef` (que fija la **magnitud** total, vía la identidad de autoconsistencia de abajo). La reflectividad corregida es `Z(r) + 2·∫[r1..r] A(s) ds` (factor 2 por el camino de ida y vuelta).

**Identidad de autoconsistencia** (el criterio de aceptación central, no sólo el sesgo): integrando `A(r)` sobre el tramo completo, `2·∫[r1,r2] A(r) dr = a_coef·ΔΦDP` exactamente, para **cualquier** forma del perfil de Z — es una propiedad algebraica de la fórmula, no un resultado empírico. `crates/attenuation` la comprueba en un test dedicado (`self_consistency_two_way_pia_matches_a_coef_times_delta_phidp`) y el oráculo hace lo mismo, precisamente porque es lo único que puede verificarse sin depender de qué tan bien `β`/`a_coef` describan la lluvia real de un caso concreto.

**Robustez numérica** (comprobada, no sólo esperada): con señal detectable en todo el tramo (`I(r1,r2) > 0`), el denominador de `A(r)` es una interpolación afín entre dos valores que son ambos positivos para cualquier `ΔΦDP` real — incluido negativo, que sólo ocurre por ruido de fase en tramos sin atenuación real. Por eso `ΔΦDP` negativo se censura a "sin atenuación medible" (`A=0` en todas las celdas) en vez de propagarse como corrección de signo equivocado, en vez de por una fragilidad del método: no hace falta, la fórmula no diverge en ese caso.

**Hallazgo al escribir el oráculo: el error por `β` mal asumido NO se degrada siempre con gracia.** Con atenuación total moderada, un `β` asumido distinto del real produce un sesgo acotado y menor que no corregir nada (ver la prueba de sensibilidad del oráculo). Pero con atenuación total ya severa (decenas de dB, un caso extremo poco realista pero no imposible de descartar a priori) un `β` bastante alejado del real puede hacer que la corrección **sobrecorrija** y termine peor que publicar la Z sin corregir — el oráculo lo reproduce (`atenuacion_zphi.ipynb`, "Prueba 4"). No es un bug de esta implementación: es una propiedad conocida del método (la relación entre Z y atenuación es no lineal en `β`, así que un error de forma se amplifica exactamente donde más atenuación hay que corregir) que cualquier consumidor de esta corrección debe tener presente al fijar `β` para su banda e instalación, no delegarlo a un valor de tabla sin verificar contra casos propios.

**Fórmula reconstruida de memoria, no del paper original.** No hay acceso a Testud et al. (2000) ni a Bringi & Chandrasekar (2001) cap. 7 en este entorno de trabajo; la fórmula de arriba se reconstruyó de la literatura conocida y **se verificó contra la implementación de Py-ART** (`pyart.correct.calculate_attenuation_zphi`, código fuente consultado por búsqueda web) para los nombres y el rol de cada coeficiente, incluida la tabla de valores por banda de abajo (atribuida por Py-ART a Gu et al. 2011). Se recomienda un contraste directo contra esa implementación (o contra el paper original) antes de tratar este crate como validado externamente — el oráculo lo señala igual.

## Configuraciones cubiertas

Existe únicamente con segundo canal (ΦDP disponible), igual que KDP: en canal único no hay forma de medir el ΔΦDP que el método necesita, así que CZ se queda como sólo "ecuación del radar + filtro de clutter" (sin el alcance nuevo de esta página). No depende del tipo de transmisor (magnetrón o coherente): igual que KDP, es una medida de fase diferencial entre canales, inmune a la fase inicial aleatoria del magnetrón.

Un rayo puede tener más de un tramo contiguo válido si hay parches de lluvia separados por celdas censuradas (ρHV bajo, sin eco, clutter sobre umbral): cada tramo se corrige de forma independiente con su propio `ΔΦDP`, tramos de una sola celda (sin intervalo que integrar) se dejan sin corregir. Ver el doc-comment de `PolarimetricValues` y el bloque que lo aplica en `crates/service::ray`.

## Parámetros del contrato que consume

De `config`, indirectamente: `gate_spacing_m` (paso de la integral) y `phidp_offset_deg` (ya aplicado aguas arriba, en la ΦDP que produce `lamula_polarimetry`). Los dos coeficientes propios del método —`β` (exponente Z-atenuación) y `a_coef` (acoplamiento atenuación-fase, en dB/grado)— **no están en el contrato v0.1**: son constantes de configuración local del DSP (`ZPHI_BETA`, `ZPHI_A_COEF_DB_PER_DEG` en `crates/service::ray`), mismo tipo de hueco que la longitud de ventana de KDP. `a_coef` depende de la banda del radar; el contrato tampoco tiene un campo de banda o de longitud de onda categorizada con que elegirlo automáticamente a partir de `wavelength_m` — mismo tipo de hueco que `polarization_mode` (`roadmap.md` §"Decisiones cerradas"). Tabla de referencia (Gu et al. 2011, vía Py-ART), con `β = 0.64884` común a las tres:

| Banda | `a_coef` [dB/grado] |
| --- | --- |
| S | 0.02 |
| C | 0.08 (valor por defecto en `crates/service::ray`) |
| X | 0.31916 |

## Criterio de aceptación

Dos criterios, no uno:

1. **Autoconsistencia** (algebraica, no depende de qué tan bien el modelo describa la lluvia real): `2·∫A(r)dr` sobre el tramo completo debe coincidir con `a_coef·ΔΦDP` dentro del error de discretización del trapecio — comprobado en `crates/attenuation` sin necesidad de simulación.
2. **Sesgo en el caso de modelo acoplado**: con la atenuación "verdadera" generada por la MISMA relación Z-β que asume el método (el único caso en que puede ser exacto salvo ruido de medida y discretización), la reflectividad recuperada debe acercarse a la verdad sustancialmente más que la medida sin corregir — declarado en `crates/attenuation/tests/against_oracle.rs` con medidas simuladas de verdad (potencia vía `lamula_simulator::generate_cell`, ΦDP vía `lamula_simulator::generate_dual_pol_cell` + `lamula_kdp::unwrap_deg`, no el perfil analítico directo).

Lo que este criterio **no** cubre: qué tan bien `β`/`a_coef` fijos describen una distribución de tamaño de gota real, o el efecto de un tramo con más de un tipo de hidrometeoro (granizo, por ejemplo, con una relación Z-atenuación distinta). Eso es una limitación conocida del método en la literatura, no un hueco de esta implementación.

## Coste de cómputo

O(N) por tramo: una integral prefijo por trapecio y una segunda pasada para la corrección acumulada, ambas con sumas incrementales — mismo orden que la calibración de reflectividad o la ventana de KDP, sin FFT ni ajuste no lineal de por medio.

## Referencias abiertas / implementaciones libres

- Testud, J., Le Bouar, E., Obligis, E. & Ali-Mehenni, M. (2000), «The Rain Profiling Algorithm Applied to Polarimetric Weather Radar», *Journal of Atmospheric and Oceanic Technology* — el método original.
- Bringi, V. N., Keenan, T. D. & Chandrasekar, V. (2001), «Correcting C-Band Radar Reflectivity and Differential Reflectivity Data for Rain Attenuation: A Self-Consistent Method With Constraints», *IEEE Transactions on Geoscience and Remote Sensing* — validación en Darwin, la extensión C-band más citada.
- Bringi, V. N. & Chandrasekar, V., *Polarimetric Doppler Weather Radar: Principles and Applications*, Cambridge University Press, 2001, cap. 7 — desarrollo de referencia (no consultado directamente en este entorno, ver "Cómo funciona").
- Gu, J.-Y. et al. (2011) — coeficientes por banda (`a_coef`, `β`) citados por la implementación de Py-ART.
- [Py-ART](https://github.com/ARM-DOE/pyart) — `pyart.correct.calculate_attenuation_zphi`, la implementación abierta contra la que se verificó la fórmula de esta página (código fuente, no el paquete en sí — no instalado en este entorno).
