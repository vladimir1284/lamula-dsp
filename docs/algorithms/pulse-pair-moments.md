# Pulse-Pair y Estimación de Momentos

## Qué resuelve

Un radar meteorológico Doppler transmite una ráfaga de pulsos por rayo y recibe, para cada celda de rango, una serie temporal de muestras complejas I/Q (una por pulso). De esa serie hay que extraer tres cantidades físicas por celda: la potencia recibida (de la que se deriva la reflectividad Z), la velocidad radial media (efecto Doppler) y el ancho espectral (dispersión de velocidades dentro del volumen de resolución, ligada a turbulencia y cizalladura). El método clásico y computacionalmente más económico para estimar estas tres cantidades es el **estimador pulse-pair**, también llamado estimador de autocovarianza de retardo 1.

## Cómo funciona

El estimador se basa en la función de autocovarianza de la serie temporal `s(1), s(2), ..., s(M)` (M pulsos por rayo). La potencia total se estima como el promedio de `|s(m)|²`; la velocidad radial se obtiene de la fase del promedio de `s(m) · s*(m+1)` (autocovarianza a retardo 1), escalada por la longitud de onda y el período entre pulsos (PRT); y el ancho espectral se deriva de la relación entre las magnitudes de la autocovarianza a retardo 0 y retardo 1, bajo un modelo de espectro gaussiano. Toda la formulación cerrada de estos tres estimadores, junto con su varianza teórica en función del número de muestras independientes, está desarrollada en detalle en Doviak & Zrnić, *Doppler Radar and Weather Observations* (2ª ed., 1993), capítulo 6, que sigue siendo la referencia canónica del campo. El estimador de velocidad específico usado casi universalmente en la industria proviene de Zrnić (1977), "Spectral Moment Estimates from Correlated Pulse Pairs", el trabajo que le da nombre a la técnica.

La ventaja del pulse-pair frente a una FFT completa del espectro Doppler es el costo: requiere solo un producto complejo por par de pulsos consecutivos (O(M) en vez de O(M log M)), lo que en los años 80-90 fue decisivo y hoy sigue siendo relevante para procesar en tiempo real cientos de miles de celdas por segundo en hardware embebido. Su limitación principal es que asume un espectro Doppler unimodal aproximadamente gaussiano; en presencia de clutter superpuesto a la señal meteorológica (dos modos en el espectro) el estimador se sesga, razón por la cual el filtrado de clutter (ver [GMAP](gmap-clutter-filtering.md)) debe aplicarse *antes* de la estimación de momentos, no después.

## Relevancia para LAMULA DSP

El **Moment Estimator** del pipeline de LAMULA DSP (ver el plan de LAMULA DSP, sección de arquitectura) implementará este estimador como algoritmo primario, con la variante de estimación espectral (FFT + ajuste) reservada como modo alternativo de mayor costo para escenarios donde la relación señal/clutter lo justifique — la misma dualidad que documentan los procesadores comerciales de gama alta.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2nd ed., Academic Press, 1993 — capítulo 6 (formulación completa de los estimadores).
- Zrnić, D. S. (1977), "Spectral Moment Estimates from Correlated Pulse Pairs", *IEEE Transactions on Aerospace and Electronic Systems*.
- [Py-ART](https://github.com/ARM-DOE/pyart) — el módulo de lectura/procesamiento de momentos de Py-ART (ARM Radar Toolkit, NASA/DOE) implementa y documenta estimadores de momentos sobre datos de radar reales en Python, útil como referencia de validación numérica.
- [LROSE / RadX](https://github.com/NCAR/lrose-core) (NCAR) — el motor de procesamiento de series temporales de LROSE incluye una implementación de referencia en C++ del estimador pulse-pair y del estimador espectral, con código abierto auditable.
