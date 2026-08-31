# Calibración polarimétrica: ZDR y ΦDP de sistema

> **Oráculo en Python**: [`tools/oracles/calibracion_polarimetrica.ipynb`](../../tools/oracles/calibracion_polarimetrica.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/pol-calibration`: offset de ZDR por birdbath y ΦDP de sistema, los dos por mediana sobre un dwell, contrastada numéricamente contra el oráculo en `crates/pol-calibration/tests/against_oracle.rs`. La aplicación del offset ya vive en `crates/polarimetry` (misma resta, probada donde vive el código que la hace); los tres métodos de determinación como campañas de operador quedan fuera de este crate salvo la simetría del birdbath, la única con verdad-terreno sintética propia.

## Qué resuelve

La [calibración de reflectividad](reflectivity-calibration.md) persigue exactitud
*absoluta*: convertir potencia recibida en dBZ físicos. La calibración
polarimétrica persigue algo distinto y más exigente en un aspecto: exactitud
*relativa* entre los dos canales de recepción. ZDR es una diferencia de dos
potencias medidas por cadenas distintas, así que cualquier diferencia de ganancia
entre esas cadenas se suma directamente al valor publicado. Y la exigencia de
precisión es dura: las aplicaciones que consumen ZDR —clasificación de
hidrometeoros, estimación de tasa de lluvia— piden un sesgo por debajo de
0,1-0,2 dB, un orden de magnitud más estricto que la tolerancia habitual de 1 dB
para la calibración absoluta de Z.

Lo mismo vale, con menos dramatismo, para ΦDP: los dos canales tienen longitudes
eléctricas distintas y eso produce un desfase constante que no es del meteoro
sino del equipo, y que hay que restar antes de publicar.

## Cómo funciona

**Offset de ZDR.** Hay tres métodos y conviene tener los tres, porque miden
partes distintas de la cadena y su acuerdo o desacuerdo es diagnóstico.

El primero es la **inyección de señal de prueba** en la entrada de los dos
receptores con potencia idéntica conocida: la diferencia medida es el desbalance
de la cadena receptora, y es la parte que puede verificarse a diario de forma
automática. No cubre la antena ni la cadena de transmisión.

El segundo es el **apuntamiento vertical** —*birdbath*, con la antena a 90° de
elevación—: en lluvia, mirando hacia arriba, las gotas se ven desde abajo y su
sección eficaz es la misma en las dos polarizaciones, así que el ZDR verdadero es
cero por simetría, y todo lo que se mida es offset. Es el método de referencia
porque cubre la cadena completa, y su limitación es operativa: exige lluvia,
exige interrumpir el escaneo, y no es aplicable con antenas que no lleguen a
vertical.

El tercero es la **comparación con dispersores naturalmente conocidos**: llovizna
ligera, o la señal de eco de dispersión de Bragg en aire claro, cuyos valores de
ZDR esperados están acotados por la literatura. Es el método de respaldo para
instalaciones sin apuntamiento vertical.

**ΦDP de sistema.** Se estima como la moda o la mediana del ΦDP medido en las
primeras celdas de rango con eco meteorológico, donde la fase acumulada de
propagación es todavía despreciable. Es un valor estable que deriva lentamente, y
su seguimiento en el tiempo es un buen indicador de salud de la cadena de
recepción.

**Aislamiento de polarización cruzada.** Si la instalación publica LDR, hay que
conocer el aislamiento de la antena, porque es el que fija el suelo por debajo
del cual LDR ya no mide el meteoro. Se determina en banco o con blanco de
referencia, y entra en el DSP como límite de validez del momento, no como una
corrección.

## Configuraciones cubiertas

Aplica sólo con polarimetría. El método de inyección de señal funciona igual en
modo simultáneo y alternante. El apuntamiento vertical es el método de referencia
en los dos, pero en modo alternante mide además la simetría de la conmutación,
que en modo simultáneo no existe. El tipo de transmisor es indiferente para el
offset de ZDR medido por birdbath —cubre la cadena entera, sea cual sea la
fuente— pero con magnetrón la deriva de frecuencia desplaza la señal dentro de
los filtros de FI de los dos canales, y si esos filtros no están perfectamente
apareados el offset de ZDR pasa a depender de la sintonía. La consecuencia
práctica es que con magnetrón el offset de ZDR debe verificarse con más
frecuencia y correlacionarse con el estado del [AFC](burst-fase-afc.md).

## Parámetros del contrato que consume

De `config`: `zdr_offset_db` y `phidp_offset_deg`, que el RCP fija y el DSP
aplica. El contrato v0.1 los transporta como constantes: **el DSP aplica la
corrección, no la determina en línea.** La determinación —campañas de birdbath,
inyección de señal, seguimiento histórico— vive del lado del RCP y del operador.
Si en el futuro se quisiera que el DSP estimara el offset por sí mismo durante un
dwell de apuntamiento vertical, haría falta un mensaje de resultado de
calibración que hoy no existe en el contrato; queda registrado como alcance de
Stage 2.

## Criterio de aceptación

Sobre escenarios simulados con un desbalance de ganancia inyectado conocido, el
ZDR publicado tras aplicar el offset debe recuperar el valor verdadero dentro de
0,1 dB. Sobre un escenario de apuntamiento vertical simulado —ZDR verdadero
cero—, el procedimiento de estimación del offset debe recuperar el desbalance
inyectado. Para ΦDP, sobre un perfil con fase de sistema inyectada y fase de
propagación creciente, la estimación de la fase de sistema debe separar
correctamente las dos contribuciones. Ninguna de estas pruebas necesita hardware,
que es precisamente lo que permite tenerlas listas antes del comisionamiento.

## Coste de cómputo

Nulo en el camino de datos: aplicar el offset es una resta. El coste está en los
procedimientos de determinación, que son campañas de medida, no bucles calientes.

## Referencias abiertas / implementaciones libres

- Bringi, V. N. & Chandrasekar, V., *Polarimetric Doppler Weather Radar*, 2001 — capítulo sobre calibración de sistemas polarimétricos.
- Ryzhkov, A. V., Giangrande, S. E., Melnikov, V. M. & Schuur, T. J. (2005), «Calibration Issues of Dual-Polarization Radar Measurements», *Journal of Atmospheric and Oceanic Technology* — tratamiento sistemático de las fuentes de sesgo de ZDR y de los métodos de calibración.
- OMM/WMO, *Guide to Instruments and Methods of Observation* y guías del programa OPERA (EUMETNET) — procedimientos operativos de verificación periódica.
- [wradlib](https://github.com/wradlib/wradlib) — `wradlib.verify` y utilidades de intercomparación, aplicables al seguimiento histórico del offset.
- [Py-ART](https://github.com/ARM-DOE/pyart) — utilidades de estimación de ΦDP de sistema y de corrección de ZDR sobre datos ya adquiridos.
