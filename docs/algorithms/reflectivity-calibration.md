# Calibración de Reflectividad

> **Oráculo en Python**: [`tools/oracles/reflectivity_calibration.ipynb`](../../tools/oracles/reflectivity_calibration.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust en `crates/calibration`: conversión potencia↔dBZ con constante de radar y corrección por r², contrastada numéricamente contra el oráculo en `crates/calibration/tests/against_oracle.rs`.

## Qué resuelve

La reflectividad meteorológica (Z, en dBZ) que reportan los productos de un radar no es una medida directa: se deriva de la potencia recibida por celda de rango aplicando la ecuación del radar meteorológico, que depende de constantes del sistema (potencia transmitida, ganancia de antena, ancho de haz, longitud de pulso, pérdidas de la cadena RF, ganancia del receptor) y del rango a la celda. Un error de calibración de solo 1 dB en la cadena se traduce directamente en 1 dB de error en Z, que a su vez puede traducirse en errores de varias veces en la tasa de precipitación estimada (Z-R es una ley de potencia). Por eso la calibración de reflectividad — y su verificación continua en operación — es uno de los procesos de mayor impacto operativo de todo el pipeline, aunque no sea un "algoritmo" único sino una cadena de procedimientos.

## Cómo se estructura la cadena de calibración

La práctica estándar (documentada por organismos como la OMM/WMO y el programa europeo OPERA en sus guías de calibración de radares meteorológicos) descompone la calibración en tres bloques: (1) **calibración del transmisor y la antena** — potencia pico transmitida, pérdidas de guía de onda, ganancia y patrón de antena, medidos en banco o por métodos de campo (p.ej. comparación con blancos de referencia o con sol/luna para el patrón de antena); (2) **calibración del receptor** — ganancia y linealidad de la cadena receptora, típicamente mediante inyección de una señal de prueba de potencia conocida (generador de señal) a la entrada del receptor y verificación de que la potencia digital reportada seguida linealmente a la potencia inyectada en todo el rango dinámico; y (3) **calibración de la constante del radar** — combinación de (1) y (2) en la constante que convierte potencia recibida en Z, junto con el piso de ruido del receptor (necesario para el umbral de detección y la corrección de potencia de ruido en celdas de señal débil). En sistemas polarimétricos se añade la calibración diferencial (ZDR), que exige mayor precisión relativa entre canales H y V que la calibración absoluta de un solo canal.

En operación, la calibración no es un evento único sino un proceso continuo: se verifica mediante inyección periódica de señal de prueba (built-in test), comparación cruzada con radares vecinos o pluviómetros en superposición, y monitoreo de la estabilidad del piso de ruido y la ganancia del receptor a lo largo del tiempo (deriva térmica, envejecimiento de componentes).

## Relevancia para LAMULA DSP

El **Moment Estimator** y el **Control/Config Plane** del pipeline (ver el plan de LAMULA DSP) deben exponer los tres bloques de calibración como parámetros configurables desde el RCP (constante de radar, tabla de ganancia del receptor, piso de ruido de referencia) y soportar el modo de inyección de señal de prueba para verificación periódica — la misma estructura de tres bloques que documentan los procesadores comerciales de gama alta.

## Configuraciones cubiertas

Con **magnetrón** hay dos complicaciones que un transmisor coherente no tiene. La
potencia pico varía pulso a pulso y con el envejecimiento del tubo, así que la
medida de potencia transmitida no es una constante de instalación sino una
cantidad a seguir de forma continua a partir de la amplitud del
[burst](burst-fase-afc.md). Y la deriva de frecuencia desplaza la señal dentro de
la banda de los filtros de FI, lo que introduce una pérdida variable que se
confunde con un cambio de calibración si no se correlaciona con el estado del
AFC. Con klistrón o estado sólido, la potencia es estable y la calibración se
comporta como la teoría dice.

Con **polarimetría**, a todo lo anterior se añade la exigencia de exactitud
*relativa* entre canales, que es un orden de magnitud más estricta que la
absoluta y tiene su propia página:
[calibración polarimétrica](calibracion-polarimetrica.md).

## Parámetros del contrato que consume

De `config`: `radar_constant_db`, `receiver_gain_db` y `noise_floor_dbm`. El DSP
**aplica** estos valores, no los determina: la determinación es un procedimiento
de operador y de banco, y el contrato v0.1 no tiene mensaje de resultado de
calibración. Cada radial publica el `radar_constant_db` y el `noise_floor_dbm`
con los que se procesó, que es lo que permite al RCP rehacer la conversión a dBZ
si hiciera falta —una decisión de diseño acertada del contrato, porque hace el
producto reprocesable en vez de irreversible.

## Criterio de aceptación

Con potencia conocida inyectada en el simulador y constante de radar conocida, la
reflectividad publicada debe coincidir con el valor analítico previsto por la
ecuación del radar, celda a celda y a lo largo de todo el alcance —lo que
verifica de paso la corrección por `r²` y, si se aplica, la de atenuación
atmosférica—. La linealidad se comprueba barriendo la potencia inyectada sobre
todo el rango dinámico y exigiendo desviación acotada respecto de la recta
teórica, que es exactamente lo que reproduce en simulación el procedimiento de
inyección de señal de prueba del sistema real.

## Coste de cómputo

Nulo en la práctica: la constante entra como un término aditivo en dB y la
corrección por rango es una tabla precalculada de un valor por celda. La
calibración no es un problema de cómputo, es un problema de procedimiento y de
trazabilidad.

## Referencias abiertas / implementaciones libres

- OMM/WMO, *Guide to Instruments and Methods of Observation*, volumen sobre radares meteorológicos — guía de referencia de procedimientos de calibración.
- Programa OPERA (EUMETNET) — guías y reportes técnicos de calibración de radares meteorológicos operativos en Europa.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2nd ed., 1993 — capítulo sobre la ecuación del radar meteorológico y calibración del sistema.
- [Py-ART](https://github.com/ARM-DOE/pyart) — módulo `pyart.correct` con utilidades de corrección de atenuación y calibración de reflectividad sobre datos ya adquiridos, útil como referencia de post-proceso y de verificación cruzada.
- [wradlib](https://github.com/wradlib/wradlib) — utilidades de calibración e intercomparación radar-pluviómetro (`wradlib.verify`) útiles para diseñar el proceso de verificación continua.
