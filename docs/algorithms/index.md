# Algoritmos de Procesamiento

Esta sección documenta, con base en literatura pública e implementaciones abiertas, los algoritmos de procesamiento de señal que **LAMULA DSP** debe implementar para alcanzar un nivel de detección y calidad de momento comparable al de un procesador de radar meteorológico líder de mercado (clase RVP900).

El listado de "qué algoritmos debe cubrir un procesador de este nivel" se derivó de estudiar la tabla de contenidos y el alcance funcional de manuales de productos comerciales establecidos. La **implementación** de cada algoritmo, en cambio, se basa exclusivamente en papers académicos, libros de referencia del dominio y proyectos open-source citados en cada página — nunca en el texto propietario de un fabricante.

| Algoritmo | Qué resuelve |
| --- | --- |
| [Pulse-pair y estimación de momentos](pulse-pair-moments.md) | Reflectividad, velocidad radial y ancho espectral a partir de la serie temporal I/Q |
| [Filtrado de clutter GMAP](gmap-clutter-filtering.md) | Supresión de eco fijo (terreno) preservando la señal meteorológica cercana a velocidad cero |
| [Dealiasing dual-PRF](dual-prf-dealiasing.md) | Extensión del intervalo de velocidad no ambigua más allá del límite de Nyquist de un único PRF |
| [Recuperación de segundo trip (SZ)](sz-second-trip-recovery.md) | Separación de ecos superpuestos en rango mediante codificación de fase aleatoria |
| [Calibración de reflectividad](reflectivity-calibration.md) | Trazabilidad de la reflectividad medida a un valor físico en dBZ |

Cada página incluye una sección de referencias abiertas con proyectos open-source (Py-ART, wradlib, LROSE/RadX) donde puede estudiarse una implementación de referencia del algoritmo o de un análogo cercano.
