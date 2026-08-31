# Mapas de clutter

> **Oráculo en Python**: [`tools/oracles/mapas_de_clutter.ipynb`](../../tools/oracles/mapas_de_clutter.ipynb) — derivado del paper, no de ningún código Rust (ver `roadmap.md` §"Método de estudio"). Implementación Rust pendiente.

## Qué resuelve

El eco de terreno no está repartido al azar: está donde están las montañas, los
edificios y las torres, y eso es conocido y estable. Un filtro adaptativo como
[GMAP](gmap-clutter-filtering.md) funciona sin saberlo, pero funciona mejor —y
con menos daño colateral sobre la señal meteorológica— si se le dice dónde
esperar clutter y dónde no. Y en las celdas donde nunca hay clutter, la opción
más segura es no filtrar en absoluto: todo filtro cuesta varianza y sesgo, y
aplicarlo donde no hace falta es pagar sin recibir nada.

## Cómo funciona

**Generación.** Un mapa de clutter es un arreglo de celdas —azimut por rango, por
elevación— con un valor por celda que indica la intensidad de clutter esperada.
Se construye acumulando observaciones en condiciones de cielo despejado: se
promedia la reflectividad de muchos barridos sin precipitación y, opcionalmente,
se registra también la variabilidad temporal de cada celda, que es lo que separa
el clutter de tierra —muy estable de barrido a barrido— del eco meteorológico
—que cambia—. Ese segundo estadístico es lo que hace utilizable el mapa: un mapa
basado sólo en la potencia media capta cualquier eco persistente, incluido el que
no es clutter.

La generación necesita cuidado con dos cosas. Primera, la propagación anómala:
en ciertas condiciones de refracción el haz se curva hacia el suelo e ilumina
terreno que normalmente no ve, lo que produce clutter *transitorio* que no debe
entrar en un mapa estático —de ahí el criterio de exigir persistencia a lo largo
de muchas observaciones separadas en el tiempo—. Segunda, la dependencia con el
ángulo de elevación: el mapa es tridimensional, y un mapa tomado a la elevación
más baja no vale para la siguiente.

**Aplicación.** El mapa entra en el pipeline como entrada auxiliar y gobierna la
agresividad del filtro celda a celda: sin clutter esperado, filtro desactivado;
con clutter esperado, filtro activo y con la anchura espectral asumida
(`clutter_width_ms`) ajustada a lo que el mapa indique. La alternativa moderna es
prescindir del mapa estático y decidir por celda y por barrido con un detector
—la familia CMD (*Clutter Mitigation Decision*)— que combina la variabilidad
espacial de la reflectividad, la posición espectral de la potencia y, si hay
polarimetría, el ρHV. Un detector así se adapta a la propagación anómala, que es
justo lo que un mapa estático no puede hacer.

La recomendación de diseño es la jerarquía que el plan del DSP ya enuncia: mapa
estático como capa base y barata, filtro espectral adaptativo como mecanismo
principal, y el detector dinámico como mejora de Stage 2 cuando haya datos reales
con los que ajustarlo, porque un detector sin ajustar es peor que un mapa.

**Ciclo de vida.** El mapa no es una constante: cambia con la vegetación, con la
construcción y con la instalación de estructuras nuevas. Necesita procedimiento
de regeneración periódica, versionado y trazabilidad de qué mapa estaba vigente
al procesar qué volumen —esto último importa cuando se investiga a posteriori un
producto anómalo.

## Configuraciones cubiertas

Independiente del transmisor. Con polarimetría, la calidad del mapa mejora
sustancialmente porque ρHV distingue clutter de meteoro mucho mejor que la sola
persistencia de la potencia, y esa mejora es la puerta natural al detector
dinámico. En canal único el mapa descansa sólo en estadísticos de potencia.

## Parámetros del contrato que consume

De `config`: `clutter_filter` selecciona el filtro y `clutter_width_ms` fija la
anchura espectral asumida del clutter. El contrato v0.1 **no transporta el mapa**:
es un fichero de configuración local del DSP, cargado en fase de configuración.
Si en el futuro se quiere que el RCP suba mapas o dispare su regeneración, hace
falta un mensaje nuevo y una v0.2; queda registrado como alcance de Stage 2.
Publica `ray_flag.clutter_filtered` y el momento `ccor` como evidencia de lo que
el filtro hizo.

## Criterio de aceptación

Sobre escenarios simulados con clutter inyectado en celdas conocidas, el
procedimiento de generación debe reconstruir el mapa verdadero con tasas de
detección y falsa alarma declaradas, incluyendo un caso con eco meteorológico
persistente que **no** debe acabar en el mapa. En la aplicación, la comprobación
que importa es la de daño colateral: en celdas marcadas sin clutter, los momentos
deben ser idénticos a los del pipeline con el filtro desactivado; en celdas con
clutter, la mejora debe medirse contra la verdad-terreno del meteoro subyacente.

## Coste de cómputo

La aplicación es una consulta por celda: despreciable, y ahorra trabajo, porque
desactivar el filtro donde no hace falta evita la FFT de esa celda. La generación
es un proceso fuera de línea sobre barridos acumulados y no compite con el tiempo
real. El coste real es de memoria: un mapa tridimensional a resolución completa
es del mismo orden que un volumen de datos, y conviene decidir pronto si se
almacena a resolución reducida.

## Referencias abiertas / implementaciones libres

- Doviak, R. J. & Zrnić, D. S., *Doppler Radar and Weather Observations*, 2ª ed., 1993 — capítulo 3: mecanismos de eco de tierra y propagación anómala.
- Hubbert, J. C., Dixon, M. & Ellis, S. M. (2009), «Weather Radar Ground Clutter. Part II: Real-Time Identification and Filtering», *Journal of Atmospheric and Oceanic Technology* — el detector dinámico tipo CMD y su comparación con el mapa estático.
- Siggia, A. D. & Passarelli, R. E. (2004), «Gaussian Model Adaptive Processing (GMAP)», *Proceedings of ERAD 2004* — el filtro al que el mapa gobierna.
- [wradlib](https://github.com/wradlib/wradlib) — `wradlib.clutter` implementa detectores de clutter por textura y por acumulación estadística, directamente aplicables a la generación del mapa.
- [LROSE](https://github.com/NCAR/lrose-core) — incluye una implementación abierta de identificación de clutter en tiempo real de la familia CMD.
