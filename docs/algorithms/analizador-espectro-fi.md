# Analizador de espectro de FI

> **Oráculo en Python**: [`tools/oracles/analizador_espectro_fi.ipynb`](../../tools/oracles/analizador_espectro_fi.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust pendiente.

## Qué resuelve

El operador y el técnico de mantenimiento necesitan ver el espectro de la señal
de frecuencia intermedia tal como llega al receptor: para verificar la sintonía
del transmisor, para localizar una fuente de interferencia, para comprobar que un
filtro está donde debe y para diagnosticar un receptor sordo. Es la herramienta
de diagnóstico que evita tener que llevar un analizador de espectro físico a la
torre.

No es un algoritmo de producto —no produce ningún momento— pero el contrato le
dedica un mensaje propio, `spectrum_frame`, y por tanto tiene que existir.

## Cómo funciona

Es un periodograma promediado, el método clásico de Welch. Se toman bloques de
muestras I/Q del canal seleccionado, se les aplica una ventana, se calcula la FFT
de cada bloque, se convierte a potencia y se promedian varias trazas en potencia
—no en dB, que es el error habitual: promediar decibelios no da la media de la
potencia sino su media geométrica, y sesga la traza hacia abajo—. El resultado se
convierte a dB al final y se escala al nivel de referencia con la ganancia de
receptor y la calibración conocidas, para que el eje vertical sea dBm reales y no
unidades arbitrarias.

Los parámetros que gobiernan la traza son los mismos de cualquier analizador: el
número de puntos fija la resolución en frecuencia, el número de promedios fija la
suavidad frente al ruido y la latencia de refresco, la ventana fija el compromiso
entre resolución y lóbulos laterales —Hann como opción sensata por defecto—, y el
span y la frecuencia central se derivan de la frecuencia de muestreo y de la
sintonía del NCO del DRx.

La única decisión con miga es de dónde se toman las muestras. Tomarlas del mismo
flujo de rayos que alimenta el pipeline es lo más barato, pero entonces la traza
sólo existe cuando hay adquisición en marcha y está condicionada por la ventana
de rango configurada. Tomarlas de un modo dedicado da libertad de configuración
pero interrumpe la adquisición. La recomendación es la primera vía como modo
normal —captura oportunista sobre el flujo vivo, sin perturbar nada— con la
posibilidad de seleccionar el tramo de muestras dentro del rayo, que cubre tanto
el burst como el ruido de fondo de las últimas celdas.

## Configuraciones cubiertas

Independiente de la polarimetría, con el canal seleccionable —el mensaje
`spectrum_frame` lleva el campo `channel` justamente para eso, y con dos canales
la comparación entre las dos trazas es en sí misma un diagnóstico de
apareamiento—. Con magnetrón, el analizador aplicado sobre el tramo de burst es
la forma directa de ver la deriva de frecuencia del transmisor y de verificar
visualmente que el [AFC](burst-fase-afc.md) está haciendo su trabajo, que es
probablemente su uso más valioso en esa configuración.

## Parámetros del contrato que consume

Publica `spectrum_frame` con `n_bins`, `channel`, `center_freq_hz`, `span_hz` y
`ref_level_dbm`, seguido de `n_bins` valores f32 en dB de menor a mayor
frecuencia. Se dispara por mandato del plano de control. El número de promedios y
la ventana no están en el contrato v0.1: son configuración local, con la misma
nota que en [KDP](kdp-estimacion.md) y [RFI](rfi-filtrado.md) sobre lo que
costaría exponerlos.

## Criterio de aceptación

Con un tono de frecuencia y potencia conocidas inyectado en el simulador, el pico
de la traza debe aparecer en el bin correcto —dentro de un bin— y con el nivel
correcto en dBm dentro del margen declarado, para varias posiciones dentro del
span, incluido el borde. Con ruido blanco de potencia conocida, el nivel medio de
la traza debe corresponder a la densidad espectral esperada teniendo en cuenta la
ganancia de la ventana, que es la comprobación que detecta el error de
normalización más común. Y la dispersión de la traza debe reducirse con el número
de promedios en el factor teórico.

## Coste de cómputo

Una FFT de `n_bins` por traza promediada, a una tasa de refresco de interfaz de
usuario —unas pocas por segundo como mucho—. Es despreciable frente al pipeline
de momentos y no compite por el presupuesto de tiempo real, siempre que la
captura no bloquee el camino de datos.

## Referencias abiertas / implementaciones libres

- Welch, P. D. (1967), «The Use of Fast Fourier Transform for the Estimation of Power Spectra», *IEEE Transactions on Audio and Electroacoustics* — el método de promediado.
- Harris, F. J. (1978), «On the Use of Windows for Harmonic Analysis with the Discrete Fourier Transform», *Proceedings of the IEEE* — ganancia de ventana y normalización, que es donde se cometen los errores de nivel.
- [GNU Radio](https://github.com/gnuradio/gnuradio) — sus bloques de estimación espectral son la referencia abierta más directa para la normalización y el escalado a dBm de una traza de este tipo.
