# Corrección de atenuación por lluvia (Z-PHI)

> **Oráculo en Python**: [`tools/oracles/atenuacion_zphi.ipynb`](../../tools/oracles/atenuacion_zphi.ipynb) — implementa la fórmula de esta página (ver "Cómo funciona" para el desarrollo completo, verificado directamente contra el paper original 2026-09-18), no de ningún código Rust (`roadmap.md` §"Método de estudio"). Implementación Rust en `crates/attenuation`: perfil de atenuación específica cerrado (formulación simple, Ecs. 19-24) y su variante acorde para banda S (Ecs. 26-27, resuelta por bisección) y reflectividad corregida, contrastados numéricamente contra el oráculo en `crates/attenuation/tests/against_oracle.rs`. Cableado en el campo CZ de `crates/service::ray` sobre cada tramo contiguo con segundo canal válido, usando la formulación acorde con los coeficientes de banda S de la instalación de referencia.

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

**Verificado 2026-09-18 contra el paper original (Testud et al. 2000).** El usuario aportó el PDF (mismo procedimiento que `5algor.pdf` para la varianza de pulse-pair, `roadmap.md`). La fórmula de arriba coincide exactamente con las ecuaciones (19), (20), (23) y (24) del paper — `A(r) = A(r0)·Za(r)^β / [Za(r0)^β + A(r0)·I(r,r0)]` con `I(r,r0) = 0.46β∫_r^{r0} Za(s)^β ds` (Ec. 19-20), resuelto para el extremo de referencia con `A(r0) = Za(r0)^β·{10^(0.1βgΔΦ)−1}/I(r1,r0)` (Ec. 23) y sustituido de vuelta para dar la forma cerrada en `A(r)` que implementa `zphi_specific_attenuation` (Ec. 24) — con `g` = `a_coef` de este crate. La corrección de reflectividad de este crate (`Z(r) + 2·∫_{r1}^{r} A(s) ds`) no es literalmente la Ec. (25) del paper (que da una forma cerrada algebraica de `Ze(r0)` vía la relación Z-atenuación), pero es **equivalente por derivación directa**: diferenciando la Ec. (19) respecto a `r` se obtiene `A(r) = -(1/0.46β)·d[ln D(r)]/dr` con `D(r)` el denominador de (19), de donde `∫_{r1}^{r0} A dr = (1/0.46β)·ln[D(r1)/D(r0)]`; convirtiendo a dB (`10/β · log10(x) = 2·(1/0.46β)·ln(x)`, porque `0.46 = 0.2·ln10`) y sustituyendo `D(r1)/D(r0)` se recupera exactamente la Ec. (25) — mismo resultado, sólo que el crate integra `A(r)` numéricamente (trapecio) en vez de la forma cerrada, lo cual es más simple de implementar y ya reutiliza `A(r)` que de todos modos se calcula. La identidad de autoconsistencia (`2·∫A dr = a_coef·ΔΦDP`) también se deriva directamente del paper: Ec. (21) fuerza `A = g·KDP` (la "formulación simple", exponente de la relación KDP-A forzado a 1) e integrando con `KDP = (1/2)·dΦDP/dr` da `∫A dr = g·ΔΦDP/2`, es decir `2∫A dr = g·ΔΦDP` — exacto, no aproximado.

**Instalación de referencia: banda S — corregido 2026-09-18.** Esta página y las constantes de `crates/service::ray` documentaban banda C por defecto sin haber verificado contra la instalación real; la instalación de referencia de este repositorio es banda S. Esto importa porque el propio paper (Tabla 1c, §c "A more accurate formulation") da el exponente de la relación KDP-A por banda — `b ≈ 0.97`–`0.99` en X/C, pero `b ≈ 1.18` en banda S — y advierte explícitamente que a banda S **no** conviene forzar ese exponente a 1 (la "formulación simple" de arriba, Ecs. 19-24): hay que usar la formulación acoplada (Ecs. 26-27). Por eso `crates/service::ray` usa en banda S la **formulación más precisa** (ver subsección de abajo), no la simple.

### Formulación más precisa (banda S, Ecs. 26-27) — agregada 2026-09-18

La formulación simple elimina algebraicamente el parámetro de intercepto normalizado de la DSD (`N*₀`) forzando a 1 el exponente `b` de la relación KDP-A (Ec. 21: `A=g·KDP`). Cuando ese forzado no es buena aproximación (banda S), el paper resuelve `A(r0)` y `N*₀` como sistema acoplado usando CUATRO coeficientes de su Tabla 1 en vez de dos — `β_AZ`/`a_AZ` (Tabla 1a, `A = a_AZ·(N*₀)^(1-β_AZ)·Za^β_AZ`) y `β_KDP`/`a_KDP` (Tabla 1c, `KDP = a_KDP·(N*₀)^(1-β_KDP)·A^β_KDP`):

```
Ec. 26 (N*₀ despejado de la relación A-Z en el extremo de referencia r0):
  D0 = Za(r0)^β_AZ + A(r0)·I(r1,r0)
  N*₀ = [A(r0) / (a_AZ · D0)]^{1/(1-β_AZ)}

Ec. 27 (restricción de ΔΦDP total vía la relación KDP-A, integrada con Ec. 19):
  a_KDP · N*₀^(1-β_KDP) · A(r0)^β_KDP · ∫[r1..r0] [Za(s)^β_AZ / (Za(r0)^β_AZ + A(r0)·I(s,r0))]^β_KDP ds = ΔΦDP/2
```

Eliminando `N*₀` de la Ec. 26 en la Ec. 27 queda una sola ecuación no lineal en `A(r0)`, que el paper dice "puede resolverse con una técnica numérica estándar" sin especificar cuál — `crates/attenuation::zphi_specific_attenuation_accurate` usa bisección (bracket ampliado por duplicación + 100 iteraciones), apoyada en que el residuo de la Ec. 27 es monótono creciente en `A(r0)` (verificado empíricamente en el oráculo para los coeficientes de banda S, no demostrado analíticamente para todo rango de coeficientes). Una vez resuelto `A(r0)`, el perfil `A(r)` completo sale de la Ec. 19 igual que en la formulación simple, y la corrección de reflectividad reutiliza la misma integral bidireccional (`Z(r) + 2·∫A(s)ds`) — esa parte no depende de cómo se resolvió `A(r)`.

**Hallazgo real al escribir el test de cableo (`crates/service::ray::tests::zphi_correction_recovers_attenuated_cz_on_dual_channel_radial`): la formulación acorde exige que `A_true` y `Z_true` sean consistentes entre sí vía el MISMO `N*₀`, no basta con que `A_true` y `ΔΦDP_true` sean consistentes entre sí (que es lo único que exigía la formulación simple).** Un primer intento de este test fijó `A_true=0.3 dB/km` como constante libre, sin relación con `Z_true=30 dBZ` vía la Tabla 1a — consistente sólo con la Ec. 27 (por construcción, vía `N*₀`), pero implicando para la Ec. 26 (que sí usa `Z_true` a través de la Tabla 1a) un `N*₀` de ~5×10¹⁴, absurdo frente al `N*₀` de ~8×10⁶ (Marshall-Palmer) que la Ec. 27 asumía. El solver, sin una `A(r0)` que satisfaga las dos ecuaciones simultáneamente para el `Za` medido, convergía a un valor equivocado y **sobrecorregía** (recuperaba ~43 dBZ en vez de 30 dBZ, peor que no corregir nada). Corregido generando `A_true` y `KDP_true` los dos a partir de `Z_true` vía la Tabla 1a y 1c con el mismo `N*₀` — la formulación simple no tiene este riesgo porque no usa `N*₀` en absoluto.

## Configuraciones cubiertas

Existe únicamente con segundo canal (ΦDP disponible), igual que KDP: en canal único no hay forma de medir el ΔΦDP que el método necesita, así que CZ se queda como sólo "ecuación del radar + filtro de clutter" (sin el alcance nuevo de esta página). No depende del tipo de transmisor (magnetrón o coherente): igual que KDP, es una medida de fase diferencial entre canales, inmune a la fase inicial aleatoria del magnetrón.

Un rayo puede tener más de un tramo contiguo válido si hay parches de lluvia separados por celdas censuradas (ρHV bajo, sin eco, clutter sobre umbral): cada tramo se corrige de forma independiente con su propio `ΔΦDP`, tramos de una sola celda (sin intervalo que integrar) se dejan sin corregir. Ver el doc-comment de `PolarimetricValues` y el bloque que lo aplica en `crates/service::ray`.

## Parámetros del contrato que consume

De `config`, indirectamente: `gate_spacing_m` (paso de la integral) y `phidp_offset_deg` (ya aplicado aguas arriba, en la ΦDP que produce `lamula_polarimetry`). Los coeficientes propios del método **no están en el contrato v0.1**: son constantes de configuración local del DSP, mismo tipo de hueco que la longitud de ventana de KDP. El contrato tampoco tiene un campo de banda o de longitud de onda categorizada con que elegir la banda automáticamente a partir de `wavelength_m` — mismo tipo de hueco que `polarization_mode` (`roadmap.md` §"Decisiones cerradas").

**Banda S (instalación de referencia, formulación acorde)**: `ZPHI_BETA_AZ_S_BAND`/`ZPHI_A_AZ_S_BAND`/`ZPHI_BETA_KDP_S_BAND`/`ZPHI_A_KDP_S_BAND` en `crates/service::ray`, los cuatro tomados directamente de la Tabla 1 de Testud et al. (2000), canal H, banda S:

| Coeficiente | Valor | Tabla |
| --- | --- | --- |
| `β_AZ` (relación A-Z) | 0.701 | 1a |
| `a_AZ` (relación A-Z) | 9.28×10⁻⁸ | 1a |
| `β_KDP` (relación KDP-A) | 1.18 | 1c |
| `a_KDP` (relación KDP-A) | 1.31×10³ | 1c |

**X/C band (formulación simple, si se activaran)**: tabla de referencia de Gu et al. (2011) vía Py-ART, con `β = 0.64884` común a las tres — distinta de la Tabla 1 propia de Testud et al. (que da `β` específico por banda y polarización, ~0.70-0.83, no un valor único), pero razonable ahí porque el paper mismo dice que forzar `b_KDP=1` en X/C es buena aproximación:

| Banda | `a_coef` [dB/grado] |
| --- | --- |
| S | 0.02 (no usado — banda S usa la formulación acorde de arriba) |
| C | 0.08 |
| X | 0.31916 |

## Criterio de aceptación

Dos criterios, no uno — para la formulación simple. La formulación acorde (banda S) tiene su propio par análogo:

1. **Autoconsistencia** (algebraica, no depende de qué tan bien el modelo describa la lluvia real): `2·∫A(r)dr` sobre el tramo completo debe coincidir con `a_coef·ΔΦDP` dentro del error de discretización del trapecio — comprobado en `crates/attenuation` sin necesidad de simulación. Para la formulación acorde no hay identidad algebraica cerrada equivalente (el sistema Ecs. 26-27 se resuelve numéricamente); el análogo es que la bisección converja (`f_hi >= 0` tras el bracket ampliado) — sin esto, la corrección panickea en vez de devolver un resultado silenciosamente incorrecto.
2. **Sesgo en el caso de modelo acoplado**: con la atenuación "verdadera" generada por la MISMA relación Z-β que asume el método (el único caso en que puede ser exacto salvo ruido de medida y discretización), la reflectividad recuperada debe acercarse a la verdad sustancialmente más que la medida sin corregir — declarado en `crates/attenuation/tests/against_oracle.rs` (formulación simple, con medidas simuladas de verdad: potencia vía `lamula_simulator::generate_cell`, ΦDP vía `lamula_simulator::generate_dual_pol_cell` + `lamula_kdp::unwrap_deg`, no el perfil analítico directo) y en `crates/attenuation::tests::matched_model_recovers_true_profile_accurate_s_band` (formulación acorde, perfil analítico directo, coeficientes de banda S) — este último exige generar `A_true` Y `KDP_true` desde `Z_true` con el MISMO `N*₀` (ver el hallazgo señalado en "Cómo funciona" arriba), no sólo `A_true` y `ΔΦDP_true` entre sí.

Lo que este criterio **no** cubre: qué tan bien `β`/`a_coef` fijos describen una distribución de tamaño de gota real, o el efecto de un tramo con más de un tipo de hidrometeoro (granizo, por ejemplo, con una relación Z-atenuación distinta). Eso es una limitación conocida del método en la literatura, no un hueco de esta implementación.

## Coste de cómputo

O(N) por tramo para la formulación simple: una integral prefijo por trapecio y una segunda pasada para la corrección acumulada, ambas con sumas incrementales — mismo orden que la calibración de reflectividad o la ventana de KDP, sin FFT ni ajuste no lineal de por medio. La formulación acorde (banda S) es O(N) también, pero con una constante ~100× mayor: cada iteración de la bisección de `A(r0)` recorre las `N` celdas del tramo para evaluar la Ec. 27, y la bisección corre unas 100 iteraciones (más el bracket ampliado inicial) — sin FFT ni ajuste no lineal tampoco, sólo más pasadas O(N) sobre el mismo tramo.

## Referencias abiertas / implementaciones libres

- Testud, J., Le Bouar, E., Obligis, E. & Ali-Mehenni, M. (2000), «The Rain Profiling Algorithm Applied to Polarimetric Weather Radar», *Journal of Atmospheric and Oceanic Technology*, vol. 17 — el método original, **consultado directamente** (PDF aportado por el usuario, 2026-09-18; ver "Cómo funciona" arriba).
- Bringi, V. N., Keenan, T. D. & Chandrasekar, V. (2001), «Correcting C-Band Radar Reflectivity and Differential Reflectivity Data for Rain Attenuation: A Self-Consistent Method With Constraints», *IEEE Transactions on Geoscience and Remote Sensing* — validación en Darwin, la extensión C-band más citada.
- Bringi, V. N. & Chandrasekar, V., *Polarimetric Doppler Weather Radar: Principles and Applications*, Cambridge University Press, 2001, cap. 7 — desarrollo de referencia (no consultado directamente en este entorno, ver "Cómo funciona").
- Gu, J.-Y. et al. (2011) — coeficientes por banda (`a_coef`, `β`) citados por la implementación de Py-ART.
- [Py-ART](https://github.com/ARM-DOE/pyart) — `pyart.correct.calculate_attenuation_zphi`, la implementación abierta contra la que se verificó la fórmula de esta página (código fuente, no el paquete en sí — no instalado en este entorno).
