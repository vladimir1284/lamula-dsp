# Contratos de cable

El LAMULA DSP habla por dos enlaces y ninguno de los dos formatos se escribe a
mano: cada uno tiene un esquema que es su única fuente de verdad, y de él se
genera el código de todos los lados que lo consumen. Lo que cambia entre los dos
es **quién manda**.

| Contrato | Lo posee | Esquema | Se genera para |
| --- | --- | --- | --- |
| `DRx↔DSP` v0.2 | Proyecto LAMULA DRx | `contract/schema/drx_dsp_v0_1.toml` del repositorio del DRx | C (DRx), Rust (DSP), Python (pruebas) |
| `DSP↔RCP` v1.0 | **Este proyecto** | `contract/schema/dsp_rcp_v0_1.toml` | Rust (DSP), Python (RCP), TypeScript (MMI) |

## DRx↔DSP: se consume, no se decide

El proyecto DRx congeló este contrato en su fase Z0 (decisión D-08) sin esperar
al equipo del DSP, con el argumento de que negociar sobre una v0.1 concreta es
más barato que negociar sobre nada. Aquí no se rediscute: se consume.

Los ficheros generados que le tocan al DSP viven en `contract/vendor/` como
**copias byte a byte** de la salida del DRx, ancladas por SHA-256 en
`contract/vendor/UPSTREAM.toml` junto con el commit del que salieron.
`tools/check_vendored_contract.py` falla si una copia se aparta del ancla, y
avisa —o falla, con `--strict`— si el origen se movió y aquí nadie re-vendorizó.

Ese ancla no es ceremonia. `cargo fmt` desciende por las declaraciones de
módulo, así que la primera vez que se formateó el crate reescribió el fichero
vendorizado por un espacio sobrante en un comentario. De ahí el `#[rustfmt::skip]`
en la declaración del módulo, y de ahí la comprobación de hash como segunda
barrera.

Para cambiar algo de este contrato: se pide en el proyecto DRx, allí se edita el
esquema y se sube `version_minor`, y aquí se re-vendoriza actualizando el ancla.

### La verificación que el DRx nos dejó

La documentación del DRx dice, textualmente, que su test de contrato **no**
cubre el lado Rust porque en aquel repositorio no hay toolchain de Rust, y que
ese test es responsabilidad del proyecto DSP. Está cubierto en dos mitades que
detectan fallos distintos:

- `contract/tests/test_drx_dsp_layout.py` reescribe la disposición campo por
  campo, de forma independiente del módulo generado, y comprueba tamaños, orden,
  tipos y desplazamientos efectivos. Detecta que el *fichero generado* cambió.
- `crates/contract/tests/layout.rs` comprueba con `offset_of!` y `size_of` lo
  que el *compilador* hace con ese fichero. Detecta, por ejemplo, un
  `#[repr(C, packed)]` perdido en una regeneración, que el test de Python
  aprobaría tan feliz.

En ambos la comprobación es «el desplazamiento es la suma de los anchos de los
campos anteriores», no «el desplazamiento es el que dice la estructura»: lo
segundo es una tautología y no detecta nada.

## DSP↔RCP: lo diseñamos aquí

El plan del DSP (§6) asigna la propiedad de este contrato a este proyecto, así
que aquí vive su esquema y su generador. `tools/gen_contract.py` es un fork del
generador del DRx —quien posee el contrato posee su generador— con tres
diferencias, todas consecuencia de que los consumidores son otros:

**Backend de TypeScript en vez de backend de C.** El MMI del RCP es Vue +
TypeScript. Aquí no hay ningún consumidor en C.

**Se admite coma flotante.** Es la divergencia deliberada respecto al contrato
hermano, y conviene entender por qué el DRx la prohíbe para ver por qué aquí no
aplica: el DRx necesita acuerdo bit a bit entre el compilador C de un Cortex-R5
y Rust, y los formatos flotantes no lo garantizan entre arquitecturas. En este
enlace los dos extremos son CPU de propósito general con IEEE-754. A cambio se
gana lo que el plan pide: los momentos viajan a precisión plena, y el RCP los
mapea sin copia con `numpy.frombuffer(buf, '<f4')`. La diezmación a 8 o 16 bits
es cosa del codificador Level-II del RCP, no del cable.

**Dos relojes, nunca un campo `timestamp` a secas.** La regla es del proyecto
RCP y es correcta: la hora de pared es lo que NEXRAD exige en Level-II y lo que
ORPG espera, pero salta cuando alguien disciplina el reloj, así que ordenar
radiales o medir intervalos con ella está mal. Un radial lleva por eso los dos
instantes, `acq_time_utc_ns` y `acq_monotonic_ns`; el resto de mensajes lleva
sólo hora de pared y lo dice en el nombre del campo, porque los lee un operador.
El monótono no es comparable entre procesos, así que al RCP le vale para
diferencias dentro del mismo flujo y no para casar con los suyos.

**Los ángulos van en grados, no en cuenta cruda de encoder.** Al revés que en
`DRx↔DSP`, donde el DRx no puede convertir porque las constantes de calibración
son del consumidor. Aquí el DSP ya las aplicó; devolver cuentas crudas obligaría
al RCP a duplicar esa calibración.

### Forma de los mensajes

Cabecera común de 12 B —misma forma y tamaño que la de `DRx↔DSP`, para que un
solo lector de tramas sirva en los dos enlaces— con `magic` distinto, de modo que
un cable mal conectado no pase desapercibido. Detrás, la cabecera del mensaje, y
detrás la carga útil cuando la hay.

| Mensaje | ID | Sentido | Cabecera | Carga útil |
| --- | --- | --- | --- | --- |
| `moment_ray` | 1 | DSP→RCP | 88 B | `n_moments` bloques de 16 B + `n_gates` f32 |
| `spectrum_frame` | 2 | DSP→RCP | 32 B | `n_bins` f32 en dB |
| `status` | 3 | DSP→RCP | 104 B | — |
| `bite_event` | 4 | DSP→RCP | 20 B | `text_len` bytes UTF-8 |
| `config_ack` | 5 | DSP→RCP | 8 B | — |
| `selftest_result` | 6 | DSP→RCP | 16 B | — |
| `capabilities` | 7 | DSP→RCP | 20 B | — |
| `config` | 8 | RCP→DSP | 84 B | — |
| `control` | 9 | RCP→DSP | 8 B | — |
| `selftest_request` | 10 | RCP→DSP | 8 B | — |

Los tamaños están medidos, no estimados: `crates/contract/tests/layout.rs` los
comprueba contra `size_of`.

### Qué cubre del checklist del plan

El §6.1 del plan del DSP fija capacidades que el esquema tenía que contemplar
para no descubrirlas a mitad de la implementación. Dónde aterriza cada una:

- **Configurar y arrancar son pasos distintos.** La enumeración `phase` separa
  `setup` de `running`; un `config` que llega en marcha se rechaza con
  `not_in_setup_phase`. No hay forma de colar configuración a mitad del flujo.
- **Autotest de enlace obligatorio al conectar.** `selftest_request` /
  `selftest_result`, con nonce para casar respuesta con petición.
- **La configuración se lee, no sólo se escribe.** El mandato `request_config`
  la devuelve, y `status.config_seq` dice cuál está vigente, así que el RCP
  confirma en vez de suponer.
- **Capacidades reportadas, no un bit de vivo/muerto.** `capability_flags` en
  `status` y en `selftest_result`, y el mensaje `capabilities` con las máscaras
  de momentos, dealiasing y estimadores que esta compilación soporta.
- **Completitud de datos y deriva, no sólo salud del enlace.** `bins_ok` frente
  a `bins_total`, `trigger_period_meas_ns` frente a `trigger_period_cmd_ns`, y
  suelo de ruido y offset de continua por canal, todo en `status`.
- **Filtrado de RFI como capacidad distinta del filtrado de clutter.** Campo
  `rfi_filter` en `config` y bandera de capacidad propia, separados de
  `clutter_filter`. Sigue pendiente la decisión de Phase 0 sobre si hace falta
  para el entorno del radar objetivo; el contrato no la prejuzga, sólo deja
  sitio.

El vocabulario canónico de momentos —UZ, CZ, V, W, ZDR, ΦDP, KDP, LDR, ρHV, más
SQI, CCOR, SIG y las componentes crudas I y Q— vive en la enumeración
`moment_kind`, común a los planes del DSP y del RCP. Va como máscara de bits en
un `u32`, así que el vocabulario no puede pasar de 32 entradas sin cambiar el
tipo; con 14 hay margen y hay un test que avisará el día que se cruce.

## Cómo se comprueba que las tres implementaciones coinciden

Que existan tres implementaciones no prueba nada; lo que hay que probar es que
producen los mismos bytes.

`contract/tests/test_dsp_rcp_codegen.py` compara Python con TypeScript
estructura por estructura, dando un valor **distinto a cada campo** —con todo a
cero, dos campos intercambiados producen bytes idénticos y el test queda ciego—
y hay un test que vigila que ese juego de valores no degenere.

El par Python↔TypeScript no es arbitrario. La implementación de Rust son
estructuras `#[repr(C, packed)]` sin código de serialización: su disposición
queda determinada por tamaño y desplazamientos, y eso ya lo comprueba
`layout.rs` contra el compilador. La de TypeScript es aritmética de índices
generada, donde cada `setFloat32(base + 44, …)` puede estar mal sin que nada más
lo note. Es la que necesita comparación de bytes de verdad.

## Procedimiento de cambio

Para `DSP↔RCP`:

1. Editar `contract/schema/dsp_rcp_v0_1.toml`, documentando el campo. Si hace
   falta relleno, se añade como campo `padN` explícito.
2. Subir `version_minor` (compatible) o `version_major` (rompe).
3. `make gen`.
4. Actualizar los tests de disposición: las tablas de `layout.rs` y las de
   `test_drx_dsp_layout.py` se reescriben a mano **a propósito**, así que un
   cambio de esquema tiene que tocarlas. Si el cambio pasa sin tocarlas, el test
   no estaba comprobando nada.
5. `make check`.
6. Avisar al proyecto RCP.

Para `DRx↔DSP`, el paso 1 ocurre en el otro repositorio y aquí sólo se
re-vendoriza y se sube el ancla de `UPSTREAM.toml`.

Nada de `contract/generated/` ni de `contract/vendor/` se edita a mano.
