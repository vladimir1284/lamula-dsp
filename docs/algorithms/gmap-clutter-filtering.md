# Filtrado de Clutter GMAP

## Qué resuelve

El eco de terreno fijo ("ground clutter") aparece en el espectro Doppler concentrado alrededor de velocidad radial cero, con una potencia típicamente muy superior a la de la señal meteorológica cercana. Un filtro de clutter debe eliminar esa componente sin distorsionar la señal meteorológica que, por coincidencia geométrica, también tenga velocidad radial cercana a cero (por ejemplo, precipitación moviéndose tangencialmente al haz). Los filtros clásicos de muesca (notch filters, IIR de orden bajo) simplemente anulan una banda fija alrededor de cero Hz; son baratos pero destruyen toda la señal meteorológica que caiga dentro de esa banda, sesgando la reflectividad y la velocidad estimadas cerca de v=0.

## Cómo funciona GMAP

**Gaussian Model Adaptive Processing (GMAP)** es la técnica introducida por Siggia & Passarelli en "Gaussian Model Adaptive Processing (GMAP) for Improved Ground Clutter Cancellation and Moment Calculation" (Proceedings of ERAD 2004, Copernicus GmbH) para resolver ese problema de forma adaptativa. La idea central: en vez de recortar una banda fija del espectro, GMAP (1) identifica la componente de clutter en el dominio espectral (típicamente muy angosta y centrada en cero, distinguible de un eco meteorológico más ancho), (2) **interpola** el espectro meteorológico subyacente en la región ocupada por el clutter usando un modelo gaussiano ajustado al resto del espectro no contaminado, en vez de ponerlo a cero, y (3) reconstruye la serie temporal filtrada a partir del espectro corregido. El resultado es que la señal meteorológica con velocidad cercana a cero se recupera en vez de perderse, y la varianza introducida en la estimación de momentos por el propio filtro se reduce sustancialmente frente a un notch filter clásico — el motivo por el que GMAP (o variantes equivalentes) se volvió el estándar de facto en procesadores comerciales de gama alta desde mediados de los 2000.

GMAP requiere trabajar en el dominio espectral (FFT de la serie temporal, corrección, IFFT), por lo que es más costoso computacionalmente que un filtro IIR en el dominio del tiempo, pero mucho más barato que un banco de filtros adaptativos tipo Clutter Environment Analysis (CLEAN-AP u otras variantes de descomposición). Para escenarios de clutter muy fuerte y persistente, se complementa con mapas de clutter estáticos (áreas conocidas de terreno) que ajustan la agresividad del filtro por celda.

## Relevancia para LAMULA DSP

El componente **Clutter Filter** del pipeline (ver el plan de LAMULA DSP) implementará GMAP como filtro primario en el dominio espectral, con un filtro IIR de menor costo como modo alternativo para celdas de bajo riesgo de clutter, y mapas de clutter fijos como entrada auxiliar — la misma jerarquía de tres niveles (mapa fijo → IIR → adaptativo espectral) que documentan los procesadores líderes del mercado.

## Referencias abiertas / implementaciones libres

- Siggia, A. D. & Passarelli, R. E. (2004), "Gaussian Model Adaptive Processing (GMAP) for Improved Ground Clutter Cancellation and Moment Calculation", *Proceedings of ERAD 2004*, Copernicus GmbH.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2nd ed., 1993 — fundamentos de filtrado de clutter en el dominio espectral y temporal.
- [Py-ART](https://github.com/ARM-DOE/pyart) — incluye rutinas de filtrado de clutter y despliegue de espectros Doppler útiles para validar visualmente el comportamiento de un filtro adaptativo.
- [wradlib](https://github.com/wradlib/wradlib) — biblioteca Python de procesamiento de radar meteorológico con módulos de clasificación y filtrado de clutter (`wradlib.clutter`), útil como referencia de algoritmos alternativos y de post-procesamiento por reflectividad/textura.
