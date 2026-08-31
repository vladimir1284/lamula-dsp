# Estimación de KDP

## Qué resuelve

KDP —la fase diferencial específica, en grados por kilómetro— es la derivada
respecto al rango de la fase diferencial ΦDP, dividida por dos: `KDP = ½·dΦDP/dr`.
Su interés es que, a diferencia de la reflectividad, es inmune a la atenuación,
a la calibración absoluta del radar y al bloqueo parcial del haz, porque es una
medida de fase y no de potencia. Eso la convierte en el mejor estimador de tasa
de lluvia intensa que existe, y en un discriminante de granizo.

Su problema es igual de conocido: ΦDP es una cantidad ruidosa, y derivar una
cantidad ruidosa amplifica el ruido. Estimar KDP no es «calcular una derivada»,
es un problema de regularización, y es el algoritmo más delicado de todo el
conjunto polarimétrico.

## Cómo funciona

La cadena tiene cuatro pasos y ninguno es opcional.

**Desdoblado de ΦDP.** La fase diferencial se mide módulo 360°, así que en
camino largo a través de lluvia intensa se pliega. Antes de derivar nada hay que
desdoblarla, aprovechando que físicamente sólo puede crecer con el rango: un
salto negativo grande entre celdas contiguas es un pliegue, no una caída real.

**Censura previa.** ΦDP sólo es interpretable donde hay eco meteorológico
coherente. Las celdas con ρHV bajo —clutter, eco biológico, ruido— aportan fase
aleatoria y hay que excluirlas antes de ajustar nada, no después. Ésta es la
dependencia dura de esta página con las
[covarianzas polarimétricas](polarimetria-covarianzas.md).

**Filtrado o regularización de ΦDP.** El enfoque clásico de Ryzhkov & Zrnić
(1996) es un filtro de mediana o un ajuste de mínimos cuadrados sobre una
ventana deslizante en rango, con la longitud de ventana como parámetro
principal: ventana corta da resolución espacial y ruido, ventana larga da
suavidad y borra los máximos de KDP, que son justo lo que interesa en convección.
De ahí las variantes adaptativas —Wang & Chandrasekar (2009)— que ajustan la
longitud de ventana a la intensidad local del eco, y los enfoques variacionales
o iterativos —Vulpiani et al. (2012), Maesaka et al. (2012)— que imponen
directamente la restricción física de que KDP no puede ser negativo en lluvia y
resuelven el problema inverso completo en vez de derivar punto a punto.

**Derivada.** Sobre el ΦDP ya regularizado, la pendiente por mínimos cuadrados
en la ventana, dividida por dos.

La recomendación para Stage 1 es implementar primero el enfoque de ventana con
mínimos cuadrados y longitud configurable —simple, bien documentado, verificable
contra los otros— y dejar la variante adaptativa como mejora posterior, con el
criterio de aceptación construido de forma que se pueda comparar las dos.

## Configuraciones cubiertas

Existe únicamente si hay polarimetría, en modo simultáneo o alternante. En canal
único, `kdp` no aparece en `moment_mask` de `capabilities`. No depende del tipo
de transmisor: es una medida de fase *diferencial* entre canales, así que la fase
inicial aleatoria de un magnetrón se cancela en la diferencia —una propiedad útil
que conviene tener presente, porque significa que ΦDP y KDP siguen siendo válidos
incluso si la corrección de fase falla.

## Parámetros del contrato que consume

De `config`: `phidp_offset_deg` como fase diferencial de sistema a restar, y
`gate_spacing_m` como paso de la derivada. La longitud de la ventana de ajuste
**no está en el contrato v0.1**: hoy sería una constante de configuración local
del DSP vía TOML. Si se quiere que el operador la gobierne desde el RCP, hace
falta un campo nuevo y por tanto una v0.2 del contrato; se registra aquí para
que la decisión se tome a la vista, no por omisión.

## Criterio de aceptación

Sobre perfiles de rango simulados con KDP verdadero conocido —un perfil de ΦDP
construido por integración de un KDP prescrito, más ruido de fase acorde al ρHV
y al número de pulsos— el estimador debe recuperar el perfil con sesgo acotado y
con una resolución espacial declarada. Dos pruebas discriminantes: un escalón de
KDP, que mide cuánto lo emborrona la ventana, y un tramo de KDP nulo con ruido,
donde el estimador no debe generar KDP negativo sistemático ni oscilaciones. La
comparación contra la implementación de Py-ART sobre los mismos perfiles es el
contraste externo natural, porque allí conviven varias de las variantes citadas.

## Coste de cómputo

Un ajuste de mínimos cuadrados por celda sobre una ventana de W celdas es O(W)
por celda con acumuladores incrementales, o O(1) amortizado si se implementa con
sumas deslizantes, que es como debe hacerse. Las variantes variacionales
iterativas son sustancialmente más caras y su viabilidad en tiempo real hay que
medirla antes de comprometerla, no después.

## Referencias abiertas / implementaciones libres

- Ryzhkov, A. V. & Zrnić, D. S. (1996), «Assessment of Rainfall Measurement That Uses Specific Differential Phase», *Journal of Applied Meteorology* — el enfoque clásico de ventana.
- Wang, Y. & Chandrasekar, V. (2009), «Algorithm for Estimation of the Specific Differential Phase», *Journal of Atmospheric and Oceanic Technology* — ventana adaptativa.
- Vulpiani, G. et al. (2012) y Maesaka, T., Iwanami, K. & Maki, M. (2012) — enfoques iterativo y variacional con restricción de positividad.
- Bringi, V. N. & Chandrasekar, V., *Polarimetric Doppler Weather Radar*, 2001 — capítulo sobre propagación y fase diferencial.
- [Py-ART](https://github.com/ARM-DOE/pyart) — `pyart.retrieve.kdp_maesaka`, `kdp_schneebeli` y `kdp_vulpiani`: tres implementaciones abiertas y contrastables entre sí, el mejor oráculo disponible para esta página.
- [wradlib](https://github.com/wradlib/wradlib) — `wradlib.dp` con desdoblado de ΦDP y estimación de KDP, útil sobre todo por su tratamiento del desdoblado.
