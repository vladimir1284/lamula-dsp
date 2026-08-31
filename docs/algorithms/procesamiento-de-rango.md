# Procesamiento de rango y modos de barrido

> **Oráculo en Python**: [`tools/oracles/procesamiento_de_rango.ipynb`](../../tools/oracles/procesamiento_de_rango.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/range`: asignación de gate, promediado de celda gruesa y composición de split-cut, contrastada numéricamente contra el oráculo en `crates/range/tests/against_oracle.rs`. El ensamblado de radial desde encoder SSI queda pendiente (sin oráculo todavía).

## Qué resuelve

Entre la serie temporal cruda que entrega el DRx y el radial de momentos que
consume el RCP hay un conjunto de decisiones de organización de los datos que no
son «un algoritmo» en el sentido de una fórmula, pero que determinan la
resolución, el alcance y la sensibilidad del sistema, y que si se equivocan
producen errores geométricos —eco desplazado en rango— que ninguna calidad de
estimador compensa.

Son tres cosas: cómo se mapean las muestras del DRx a celdas de rango, cómo se
promedian en rango cuando se pide celda gruesa, y cómo se combinan barridos con
parámetros distintos para obtener a la vez buena reflectividad y buena velocidad.

## Cómo funciona

**Asignación de rango.** Cada muestra tras el DDC corresponde a un instante desde
el disparo, y por tanto a una distancia `r = c·t/2`. El primer bin no está en
`r = 0`: hay un retardo de sistema —longitud de guía de onda, latencia del
receptor y del filtro digital, retardo del propio trigger— que desplaza el origen
y que tiene que medirse y configurarse, no estimarse. El contrato lo expone como
`start_range_m`, y los `trigger_delay_N` del contrato `DRx↔DSP` son la otra mitad
de la misma cuenta. Un error aquí es un error de posición de todo el eco, y es de
los que sobreviven años sin que nadie lo note si no se verifica contra un blanco
fijo de posición conocida.

**Resolución y promediado en rango.** El tamaño de celda no puede ser mejor que
la resolución que da el ancho de pulso: con un pulso de 0,83 µs la resolución
física son 125 m, y pedir celdas más finas no añade información, sólo muestras
correlacionadas. El modo de celda gruesa —`cell_mode` del contrato `DRx↔DSP`—
promedia muestras contiguas en rango, lo que reduce la varianza de los momentos
por raíz del número de muestras promediadas a costa de resolución. La decisión de
dónde se hace ese promediado importa: promediar **potencias** después de estimar
es distinto de promediar **muestras I/Q** antes de estimar, y sólo lo segundo
gana muestras independientes de verdad. Hay que fijar y documentar cuál se hace.
Con ancho de pulso variable, la tabla que relaciona `pulse_width_idx` con el
tamaño de celda coherente es configuración, y una petición incoherente se rechaza
con `gate_count_illegal` o `prf_range_illegal` según el caso.

**Modos de barrido / tipos de corte.** El compromiso entre rango no ambiguo y
velocidad no ambigua se puede resolver también repartiendo el trabajo entre
barridos distintos, y el contrato `DRx↔DSP` expone tres:

- **Split cut.** Dos barridos completos a la misma elevación: uno a PRF baja, que
  da alcance largo y reflectividad limpia, y otro a PRF alta, que da velocidad
  con Nyquist amplio y alcance corto. Los momentos publicados se componen tomando
  la reflectividad del primero y la velocidad y el ancho espectral del segundo.
  La combinación exige que las dos pasadas estén alineadas en azimut, y esa
  alineación es responsabilidad del pipeline, no de la antena.
- **Batch cut.** El mismo reparto pero dentro de un solo barrido, alternando
  bloques de pulsos de PRF baja y alta por radial. Ahorra un barrido completo de
  tiempo y evita el problema de alineación, a costa de menos pulsos por bloque y
  por tanto más varianza en ambos conjuntos de momentos.
- **Doppler cut.** Un único barrido a PRF alta; todo se estima de la misma serie.
  Es el modo simple y el que exige más del [dealiasing](dual-prf-dealiasing.md).

**Ensamblado del radial.** Los ángulos llegan como cuentas crudas de encoder SSI
—`azimuth_raw`, `elevation_raw`— y hay que convertirlos a grados con la
resolución del encoder y su offset de cero, ambos configuración. Un radial cubre
un sector angular, no un ángulo: el contrato publica `az_start_deg`/`az_end_deg`
y `el_start_deg`/`el_end_deg` precisamente para no colapsar eso en un solo
número. La decisión de cuándo se cierra un radial —por número de pulsos, por
ángulo recorrido, o por lo primero que ocurra— determina si el producto tiene
radiales de anchura uniforme, y hay que tomarla explícitamente.

## Configuraciones cubiertas

Independiente de transmisor y de polarimetría, con dos matices. En modo
polarimétrico alternante, un bloque de pulsos contiene la mitad de muestras por
canal, lo que interactúa con la elección de longitud de bloque en batch cut. Con
magnetrón, la [corrección de fase](burst-fase-afc.md) debe aplicarse antes de
cualquier promediado de muestras I/Q en rango; promediar muestras con fase
aleatoria sin corregir las destruye.

## Parámetros del contrato que consume

De `config` (`DSP↔RCP`): `start_range_m`, `gate_spacing_m`, `n_gates`,
`prf_hz`, `sweep_mode`. Del contrato `DRx↔DSP`: `bins`, `pulse_width_idx`,
`cell_mode`, `scan_mode`, `prf_div`, `azimuth_raw`, `elevation_raw` y los
`trigger_delay_N`/`trigger_width_N`. Publica por radial la geometría completa,
más `unambiguous_range_m` y `nyquist_velocity`.

## Criterio de aceptación

Un blanco puntual simulado a rango conocido debe aparecer en la celda que le
corresponde, con tolerancia de menos de media celda, en todas las combinaciones
de ancho de pulso y modo de celda soportadas. El promediado en rango debe
reducir la varianza de los momentos en el factor teórico esperado —si no lo hace,
las muestras que se están promediando no son independientes y el modo no está
dando lo que promete—. Para los modos de corte, un escenario con reflectividad y
velocidad conocidas y velocidad por encima de la Nyquist de la PRF baja debe
producir un radial compuesto con la reflectividad del barrido largo y la
velocidad correcta del corto, verificando de paso que la alineación en azimut no
introduce desplazamiento.

## Coste de cómputo

La asignación de rango es aritmética de índices. El promediado en rango es lineal
en el número de muestras y se funde en la misma pasada que la conversión de
formato y la corrección de fase. El coste real de esta etapa no es aritmético
sino de memoria: es donde se decide la disposición de los buffers —canal más
rápido que bin en el cable, según el contrato— y una disposición que obligue a
recorrer la memoria a saltos en la etapa de momentos cuesta más que todos los
cálculos de esta página juntos.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulos 3 y 4: resolución en rango, volumen de resolución y muestreo.
- Torres, S. M. & Zrnić, D. S. (2003), «Whitening in Range to Improve Weather Radar Spectral Moment Estimates», *Journal of Atmospheric and Oceanic Technology* — sobremuestreo en rango y blanqueado, la vía avanzada para ganar muestras independientes; candidata de Stage 2.
- [LROSE/RadX](https://github.com/NCAR/lrose-core) — su modelo de datos de radar (geometría de radial, sector angular, modos de barrido) es la referencia abierta más completa y coincide conceptualmente con lo que el contrato publica.
- [Py-ART](https://github.com/ARM-DOE/pyart) — su objeto `Radar` y sus convenios de geometría sirven de contraste para verificar que la geometría publicada se interpreta como se pretende aguas abajo.
