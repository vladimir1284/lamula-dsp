# Simulador de I/Q con momentos prescritos

## Qué resuelve

Toda la aceptación del LAMULA DSP en el mes 8 se hace contra el simulador de
señal: no hay FPGA disponible hasta después. Eso convierte al generador de
series temporales I/Q en el algoritmo más crítico del conjunto, aunque no
produzca ningún momento. Si el simulador genera una señal cuyos momentos
verdaderos no son exactamente los que dice generar, todos los criterios de
exactitud del proyecto miden contra una verdad falsa, y el error no se descubre
hasta el comisionamiento con hardware real.

Lo que hace falta es un generador que, dados una potencia S, una velocidad
radial media v, un ancho espectral σv y —cuando aplique— un conjunto de
variables polarimétricas, produzca una serie de M muestras complejas por celda
de rango cuya estadística sea la de un eco meteorológico real con esos momentos,
con verdad-terreno analítica conocida celda a celda.

## Cómo funciona

El método estándar es el de Zrnić (1975), «Simulation of Weakly Correlated
Signals», y su desarrollo posterior en Galati & Pavan (1995). Se parte del
modelo físico aceptado: el eco de un volumen de resolución lleno de dispersores
independientes es un proceso gaussiano complejo de media cero, cuya densidad
espectral de potencia es aproximadamente gaussiana, centrada en la velocidad
media y con anchura dada por el ancho espectral. Generar una realización de ese
proceso se reduce a tres pasos: construir el espectro objetivo `S(f)` sobre la
malla de frecuencias de Nyquist correspondiente al PRT, multiplicar punto a
punto la raíz de ese espectro por ruido blanco gaussiano complejo, y aplicar la
IFFT. El resultado es una serie temporal correlacionada con exactamente la
autocovarianza que se pidió, salvo el error estadístico propio de una
realización finita.

Sobre esa base se superponen, como componentes aditivas independientes, el resto
de los ingredientes que el DSP tiene que saber tratar: ruido térmico blanco a la
potencia que fije el suelo de ruido; una componente de clutter de tierra, que es
el mismo proceso con velocidad media cero y ancho espectral muy pequeño y
potencia muy superior; ecos de segundo y tercer trip, que son componentes
generadas para una celda de rango distinta y desplazadas en el tiempo; e
interferencia de banda estrecha, que es un tono con deriva de fase.

**Variabilidad de transmisor.** El simulador aplica al final, y de forma
opcional, la firma del transmisor. Para magnetrón: una fase inicial aleatoria
uniforme distinta por pulso, aplicada a la serie completa de esa celda, más una
muestra de burst coherente con esa misma fase para que la etapa de corrección
tenga de dónde leerla, más una deriva lenta de frecuencia que ejercite el lazo
de AFC. Para transmisor coherente: fase determinista y burst estable, con jitter
de fase acotado como caso degradado. La verdad-terreno no cambia entre los dos
casos, que es exactamente lo que permite usar el mismo criterio de aceptación
para comprobar que la corrección de fase recupera lo que el magnetrón estropeó.

**Variabilidad polarimétrica.** Con canal único basta una serie por celda. Con
dos canales hay que generar un par de series *conjuntamente* correlacionadas, no
dos series independientes: la matriz de covarianza 2×2 entre los canales H y V
queda fijada por ZDR (razón de potencias), ρHV (módulo de la correlación
copolar) y ΦDP (su fase). La generación se hace descomponiendo esa matriz —una
factorización de Cholesky basta— y aplicándola al par de series blancas antes
del filtrado espectral. En modo alternante, las muestras de cada canal se
intercalan en el tiempo en vez de coexistir, que es justo la diferencia que
obliga a estimadores distintos aguas abajo.

## Parámetros del contrato que consume

Ninguno directamente: el simulador vive detrás de la AAL y emula al DRx, así que
lo que produce son tramas `Ray` del contrato `DRx↔DSP` con su carga útil de
pares (I,Q) int16 entrelazados, más los campos de metadatos de rayo
—`prf_div`, `bins`, `n_channels`, `channel_mask`, `pulse_width_idx`,
`cell_mode`, `azimuth_raw`, `elevation_raw`—. Es su fidelidad a *ese* contrato,
y no a una API interna, lo que hace que el pipeline no distinga simulador de
hardware.

## Criterio de aceptación

El simulador se valida contra sí mismo por vía estadística, no contra el DSP:
sobre N realizaciones independientes de la misma configuración se estima la
autocovarianza muestral promedio y se compara con la autocovarianza analítica
del modelo gaussiano pedido, exigiendo que la diferencia quede dentro del error
estándar esperado para ese N. Se comprueba además que la distribución de
potencia por celda es exponencial (Rayleigh en amplitud), que la fase está
uniformemente distribuida, y que en el caso de dos canales la correlación
cruzada muestral reproduce el ρHV y el ΦDP pedidos. Este conjunto de pruebas es
lo que autoriza a llamar «verdad-terreno» a la configuración de entrada.

Un segundo criterio, más caro pero más convincente, es la comparación
cualitativa contra I/Q real registrado del Vesta legado: los histogramas de
potencia y las autocovarianzas de una celda de lluvia estratiforme real deben
ser indistinguibles, dentro de la dispersión, de las de una celda simulada con
los momentos que el procesador legado reportó para ella.

## Coste de cómputo

Una FFT y una IFFT de longitud M por celda y por canal, más la generación de
2·M números gaussianos. No es una etapa de tiempo real —el simulador puede
pregenerar escenarios a disco— pero conviene que sea rápida: la malla de
validación de los estimadores necesita del orden de decenas de miles de
realizaciones por punto de configuración.

## Referencias abiertas / implementaciones libres

- Zrnić, D. S. (1975), «Simulation of Weakly Correlated Signals», *IEEE Transactions on Geoscience Electronics* — el método base.
- Galati, G. & Pavan, G. (1995), «Computer Simulation of Weather Radar Signals», *Simulation Practice and Theory* — extensión con clutter, multi-trip y casos no gaussianos.
- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulos 4 y 6: modelo estadístico del eco meteorológico.
- Bringi, V. N. & Chandrasekar, V., *Polarimetric Doppler Weather Radar*, 2001 — matriz de covarianza polarimétrica y su simulación.
- [Py-ART](https://github.com/ARM-DOE/pyart) y [LROSE](https://github.com/NCAR/lrose-core) — no incluyen un simulador equivalente, pero sus lectores de series temporales sirven para volcar el escenario simulado a un formato que sus herramientas de despliegue sepan pintar, que es la vía práctica de inspección visual.
