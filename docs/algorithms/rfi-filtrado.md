# Filtrado de interferencia de banda estrecha (RFI)

> **Oráculo en Python**: [`tools/oracles/rfi_filtrado.ipynb`](../../tools/oracles/rfi_filtrado.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/rfi`: detección por combinación de exceso sobre la mediana (28 dB, calibrado contra el peor caso de falso positivo por comparaciones múltiples con M=256) y anchura angosta contigua al pico (máximo 3 bins, el lóbulo de la ventana de Hann, no la anchura Doppler de un eco real). La interpolación reutiliza sin reimplementar el `gmap_filter` de `crates/clutter` (mismo mecanismo de ajuste gaussiano), contrastada numéricamente contra el oráculo en `crates/rfi/tests/against_oracle.rs`, incluida la prueba de orden RFI-antes-que-clutter.

## Qué resuelve

La banda de un radar meteorológico está compartida. En banda C y sobre todo en
banda X conviven enlaces de datos, sistemas RLAN y otros radares, y su señal
entra por el receptor como potencia que no procede de ningún meteoro. El
artefacto típico es un radial o un sector de radiales con reflectividad elevada y
uniforme, sin estructura meteorológica, que un producto de acumulación de lluvia
convierte en precipitación inventada.

El contrato expone `rfi_filter` como un interruptor por configuración, así que el
DSP tiene que ofrecer algo detrás de él.

## Cómo funciona

La interferencia típica es de banda estrecha y no coherente con el eco: en el
espectro Doppler de la celda aparece como una o pocas líneas de potencia elevada
en una posición que no guarda relación con la velocidad del meteoro, y —a
diferencia del clutter, que está anclado en cero— puede aparecer en cualquier
frecuencia. Eso da la vía de detección más directa: sobre el espectro que la
etapa espectral ya calcula, se busca la línea o líneas cuya potencia excede en un
margen declarado la mediana del resto del espectro, y se sustituyen por
interpolación de sus vecinas, exactamente el mismo mecanismo de relleno que
[GMAP](gmap-clutter-filtering.md) usa para el hueco del clutter. Usar la mediana
y no la media es lo que hace robusta la detección: la media ya está contaminada
por la propia interferencia que se quiere detectar.

Una segunda familia de métodos trabaja al nivel del radial completo en vez de la
celda: la interferencia suele afectar a muchas celdas contiguas en rango con un
perfil de potencia característico —decae de forma suave con el rango, sin la
estructura del eco meteorológico—, y detectarla por el patrón del radial entero
permite marcarlo completo. Es más barata y menos precisa, y sirve como red de
seguridad cuando el filtrado espectral no está activo.

**Interacción que hay que resolver explícitamente.** El filtro de RFI y el
filtro de clutter tocan el mismo espectro. El orden importa: primero RFI —porque
una línea de interferencia dentro de la región del clutter distorsiona el ajuste
gaussiano de GMAP y arruina la interpolación—, después el clutter. Y la potencia
retirada por RFI **no** debe contarse en el CCOR, que es por definición la
corrección atribuible al clutter; mezclar las dos hace que la censura por
`ccor_threshold` descarte celdas por el motivo equivocado.

## Configuraciones cubiertas

Independiente del transmisor. Con polarimetría se aplica por canal, y la
detección puede además aprovechar que la interferencia no está correlacionada
entre canales: un ρHV anormalmente bajo junto con potencia alta es una firma de
interferencia muy fiable, y es el discriminante que un radar de canal único no
tiene. En canal único la detección descansa sólo en la forma del espectro.

## Parámetros del contrato que consume

De `config`: `rfi_filter` como interruptor. El contrato v0.1 no expone umbral de
detección; es configuración local del DSP vía TOML, y si se quisiera gobernable
desde el RCP haría falta una v0.2. Se registra igual que la ventana de KDP, para
que la decisión sea consciente.

## Criterio de aceptación

Sobre escenarios simulados con un tono inyectado de potencia y frecuencia
conocidas superpuesto a eco meteorológico de momentos conocidos, los momentos
recuperados con el filtro activo deben coincidir con los del mismo escenario sin
interferencia, dentro del margen declarado. La prueba complementaria, y la que
más importa, es la de **falsos positivos**: sobre escenarios sin interferencia
alguna, el filtro activo no debe alterar los momentos más allá de un margen
mínimo declarado. Un filtro de RFI que se dispara sobre señal limpia hace más
daño del que evita.

## Coste de cómputo

Si el estimador espectral o GMAP ya están activos, el espectro ya está calculado
y la detección añade una mediana y un recorrido lineal por celda: coste marginal
pequeño. Si no lo están, activar el filtrado de RFI obliga a pagar la FFT
completa, y ése es el coste real que hay que presupuestar. La detección por
radial es despreciable en cualquier caso.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulo 6: comportamiento espectral de señales no meteorológicas.
- Siggia, A. D. & Passarelli, R. E. (2004), «Gaussian Model Adaptive Processing (GMAP)», *Proceedings of ERAD 2004* — el mecanismo de interpolación espectral que se reutiliza aquí.
- Peura, M. (2002), «Computer Vision Methods for Anomaly Removal», *Proceedings of ERAD 2002* — detección de anomalías por patrón espacial, base de la variante por radial.
- [wradlib](https://github.com/wradlib/wradlib) — `wradlib.clutter` incluye detectores de anomalía por textura y patrón, aplicables a la variante por radial.
- Nota de honestidad sobre el estado del arte abierto: no hay una implementación abierta de referencia del filtrado espectral de RFI tan establecida como las de otros algoritmos de esta sección. La validación descansa aquí más que en ninguna otra página sobre la inyección controlada en el simulador.
