# Algoritmos de Procesamiento

Esta sección documenta, con base en literatura pública e implementaciones abiertas, los algoritmos de procesamiento de señal que **LAMULA DSP** debe implementar para alcanzar un nivel de detección y calidad de momento comparable al de un procesador de radar meteorológico líder de mercado (clase RVP900).

El listado de "qué algoritmos debe cubrir un procesador de este nivel" se derivó de estudiar la tabla de contenidos y el alcance funcional de manuales de productos comerciales establecidos, y de recorrer perilla a perilla lo que el contrato `DSP↔RCP` v0.1 ya promete al RCP. La **implementación** de cada algoritmo, en cambio, se basa exclusivamente en papers académicos, libros de referencia del dominio y proyectos open-source citados en cada página — nunca en el texto propietario de un fabricante.

**Empieza por el [plan de estudio e implementación](roadmap.md)**: fija el método de trabajo (oráculo en Python, luego Rust, luego contraste numérico), el criterio de aceptación común (varianza teórica, no sólo sesgo), el orden por fases y las decisiones que siguen abiertas.

## Los dos ejes de variabilidad del hardware

El DSP no asume una configuración concreta de radar. Todo el conjunto se diseña sobre dos ejes independientes, y cada página declara qué cambia en cada combinación: **transmisor** (magnetrón, que obliga a corrección de fase y AFC, frente a klistrón o estado sólido, coherentes) y **polarimetría** (canal único, simultánea STAR, o alternante H/V con LDR). El conjunto de momentos y modos que una instalación produce es una capacidad en tiempo de ejecución declarada vía `capabilities`, no una constante de compilación.

## Cadena de señal

| Algoritmo | Qué resuelve |
| --- | --- |
| [Simulador de I/Q](simulador-iq.md) | Series temporales con momentos prescritos y verdad-terreno analítica; base de toda la validación hasta el comisionamiento |
| [Burst, corrección de fase y AFC](burst-fase-afc.md) | Coherencia de la serie temporal con transmisor de magnetrón, y control automático de frecuencia |
| [Procesamiento de rango y modos de barrido](procesamiento-de-rango.md) | Mapeo muestra-a-celda, promediado en rango, split/batch/doppler cut, ensamblado del radial |
| [Ruido, resta y umbrales](ruido-y-umbrales.md) | Suelo de ruido, resta, y las cuatro censuras que evitan publicar lluvia inventada |
| [Filtrado de RFI](rfi-filtrado.md) | Interferencia de banda estrecha de otros emisores en la banda |
| [Mapas de clutter](mapas-de-clutter.md) | Dónde esperar eco fijo, y no filtrar donde no hace falta |
| [Filtrado de clutter GMAP](gmap-clutter-filtering.md) | Supresión de eco fijo preservando la señal meteorológica cercana a velocidad cero |

## Estimación de momentos

| Algoritmo | Qué resuelve |
| --- | --- |
| [Pulse-pair y estimación de momentos](pulse-pair-moments.md) | Reflectividad, velocidad radial y ancho espectral a partir de la serie temporal I/Q |
| [Estimador espectral](estimador-espectral.md) | Alternativa de mayor coste para espectros multimodales o con clutter residual |
| [Índices de calidad](indices-de-calidad.md) | SQI, CCOR y SIG: el contexto sin el cual un momento es un número suelto |
| [Calibración de reflectividad](reflectivity-calibration.md) | Trazabilidad de la reflectividad medida a un valor físico en dBZ |
| [Corrección de atenuación Z-PHI](atenuacion-zphi.md) | Recuperar Z tras lluvia intensa usando ΔΦDP como restricción, sin necesitar la constante de la relación Z-atenuación |

## Polarimetría

| Algoritmo | Qué resuelve |
| --- | --- |
| [Variables polarimétricas](polarimetria-covarianzas.md) | ZDR, ρHV, ΦDP y LDR desde la matriz de covarianza entre canales; los tres modos de polarización |
| [Estimación de KDP](kdp-estimacion.md) | Fase diferencial específica: inmune a atenuación y calibración, y el estimador más delicado del conjunto |
| [Calibración polarimétrica](calibracion-polarimetrica.md) | Offset de ZDR y fase diferencial de sistema, con exigencia de exactitud relativa de 0,1 dB |

## Ambigüedades

| Algoritmo | Qué resuelve |
| --- | --- |
| [Dealiasing dual-PRF](dual-prf-dealiasing.md) | Extensión del intervalo de velocidad no ambigua alternando PRF entre radiales |
| [Staggered-PRT](staggered-prt.md) | Lo mismo alternando el periodo entre pulsos consecutivos, sin error por decorrelación |
| [Dealiasing de rango](dealiasing-de-rango.md) | Ecos de trip múltiple: detección y marcado, o recuperación según lo que permita el transmisor |
| [Recuperación de segundo trip (SZ)](sz-second-trip-recovery.md) | Separación de ecos superpuestos mediante codificación de fase sistemática; Stage 2 |

## Diagnóstico

| Algoritmo | Qué resuelve |
| --- | --- |
| [Analizador de espectro de FI](analizador-espectro-fi.md) | Traza espectral bajo demanda para sintonía, interferencia y diagnóstico de receptor |

Cada página incluye una sección de referencias abiertas con proyectos open-source (Py-ART, wradlib, LROSE/RadX) donde puede estudiarse una implementación de referencia del algoritmo o de un análogo cercano, y declara los parámetros del contrato que consume y el criterio numérico con el que se dará por terminada.
