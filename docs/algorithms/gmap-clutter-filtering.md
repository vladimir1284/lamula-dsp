# Filtrado de Clutter GMAP

> **Oráculo en Python**: [`tools/oracles/gmap_clutter_filtering.ipynb`](../../tools/oracles/gmap_clutter_filtering.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust pendiente.

## Qué resuelve

El eco de terreno fijo ("ground clutter") aparece en el espectro Doppler concentrado alrededor de velocidad radial cero, con una potencia típicamente muy superior a la de la señal meteorológica cercana. Un filtro de clutter debe eliminar esa componente sin distorsionar la señal meteorológica que, por coincidencia geométrica, también tenga velocidad radial cercana a cero (por ejemplo, precipitación moviéndose tangencialmente al haz). Los filtros clásicos de muesca (notch filters, IIR de orden bajo) simplemente anulan una banda fija alrededor de cero Hz; son baratos pero destruyen toda la señal meteorológica que caiga dentro de esa banda, sesgando la reflectividad y la velocidad estimadas cerca de v=0.

## Cómo funciona GMAP

**Gaussian Model Adaptive Processing (GMAP)** es la técnica introducida por Siggia & Passarelli en "Gaussian Model Adaptive Processing (GMAP) for Improved Ground Clutter Cancellation and Moment Calculation" (Proceedings of ERAD 2004, Copernicus GmbH) para resolver ese problema de forma adaptativa. La idea central: en vez de recortar una banda fija del espectro, GMAP (1) identifica la componente de clutter en el dominio espectral (típicamente muy angosta y centrada en cero, distinguible de un eco meteorológico más ancho), (2) **interpola** el espectro meteorológico subyacente en la región ocupada por el clutter usando un modelo gaussiano ajustado al resto del espectro no contaminado, en vez de ponerlo a cero, y (3) reconstruye la serie temporal filtrada a partir del espectro corregido. El resultado es que la señal meteorológica con velocidad cercana a cero se recupera en vez de perderse, y la varianza introducida en la estimación de momentos por el propio filtro se reduce sustancialmente frente a un notch filter clásico — el motivo por el que GMAP (o variantes equivalentes) se volvió el estándar de facto en procesadores comerciales de gama alta desde mediados de los 2000.

GMAP requiere trabajar en el dominio espectral (FFT de la serie temporal, corrección, IFFT), por lo que es más costoso computacionalmente que un filtro IIR en el dominio del tiempo, pero mucho más barato que un banco de filtros adaptativos tipo Clutter Environment Analysis (CLEAN-AP u otras variantes de descomposición). Para escenarios de clutter muy fuerte y persistente, se complementa con mapas de clutter estáticos (áreas conocidas de terreno) que ajustan la agresividad del filtro por celda.

## Relevancia para LAMULA DSP

El componente **Clutter Filter** del pipeline (ver el plan de LAMULA DSP) implementará GMAP como filtro primario en el dominio espectral, con un filtro IIR de menor costo como modo alternativo para celdas de bajo riesgo de clutter, y mapas de clutter fijos como entrada auxiliar — la misma jerarquía de tres niveles (mapa fijo → IIR → adaptativo espectral) que documentan los procesadores líderes del mercado.

## Configuraciones cubiertas

Independiente del transmisor, con la condición habitual del magnetrón: la
corrección de fase tiene que haberse aplicado antes, o el espectro sobre el que
GMAP busca el clutter no existe.

Con polarimetría hay una decisión que sesga ZDR si se toma mal: el filtro se
aplica a los dos canales, pero **la decisión de filtrar y la anchura del hueco
tienen que ser la misma para ambos**. Filtrar cada canal por separado con
decisiones independientes hace que en unas celdas se quite potencia de H y no de
V, y esa diferencia se publica como ZDR sin serlo. La regla es: decisión
conjunta, aplicación por canal.

## Parámetros del contrato que consume

De `config`: `clutter_filter` (`none`, `gmap` o `notch`), `clutter_width_ms` como
anchura espectral asumida del clutter, `ccor_threshold` para la censura por
corrección excesiva y `n_pulses` como longitud de FFT. Consume además el
[mapa de clutter](mapas-de-clutter.md) como entrada auxiliar. Publica el momento
`ccor`, la bandera `ray_flag.clutter_filtered` y `moment_flag.filtered` en los
bloques afectados.

## Criterio de aceptación

Tres escenarios y los tres son necesarios. Primero, clutter inyectado de potencia
conocida sobre meteoro de momentos conocidos, barriendo la razón clutter-señal:
los momentos recuperados deben seguir la verdad-terreno dentro del margen
declarado, y el criterio se expresa como curva frente a esa razón. Segundo, y es
el que justifica GMAP frente a un notch, meteoro con velocidad radial cercana a
cero superpuesto al clutter: ahí un notch destruye la señal y GMAP debe
recuperarla, y la diferencia entre ambos es el resultado que hay que medir y
publicar. Tercero, ausencia de clutter con el filtro activo: los momentos no
deben degradarse más allá de un margen mínimo declarado.

## Coste de cómputo

Una FFT y una IFFT de longitud `n_pulses` por celda y por canal, más el ajuste
gaussiano: el mismo orden que el [estimador espectral](estimador-espectral.md), y
la etapa más cara del pipeline cuando está activa en todas las celdas. Es la
razón práctica de tener mapa de clutter: desactivar el filtro donde no hace falta
es la optimización de mayor rendimiento disponible.

## Referencias abiertas / implementaciones libres

- Siggia, A. D. & Passarelli, R. E. (2004), "Gaussian Model Adaptive Processing (GMAP) for Improved Ground Clutter Cancellation and Moment Calculation", *Proceedings of ERAD 2004*, Copernicus GmbH.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2nd ed., 1993 — fundamentos de filtrado de clutter en el dominio espectral y temporal.
- [Py-ART](https://github.com/ARM-DOE/pyart) — incluye rutinas de filtrado de clutter y despliegue de espectros Doppler útiles para validar visualmente el comportamiento de un filtro adaptativo.
- [wradlib](https://github.com/wradlib/wradlib) — biblioteca Python de procesamiento de radar meteorológico con módulos de clasificación y filtrado de clutter (`wradlib.clutter`), útil como referencia de algoritmos alternativos y de post-procesamiento por reflectividad/textura.
