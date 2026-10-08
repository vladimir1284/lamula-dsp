# Despliegue

`docs/dsp-plan.md` §3.1 pone en alcance Stage 1 un "offline, air-gapped
installer for the Linux SBC (systemd service); reproducible offline build".
Esta página documenta lo que existe hoy en `packaging/` y `tools/` para
eso — y, con la misma honestidad que el resto de la documentación de este
repositorio, lo que todavía no decide ni resuelve.

## Build offline reproducible

`tools/offline_build.sh` separa el proceso en dos fases deliberadamente
distintas:

1. **`tools/offline_build.sh vendor`** — con acceso a red, vendoriza todas
   las dependencias de `Cargo.lock` en un árbol local (`vendor/`, ignorado
   por git) y genera la config de Cargo que apunta a él en vez de a
   crates.io.
2. **`tools/offline_build.sh build`** — sin red (`--offline`), compila
   `lamula-dsp` en modo `release` exclusivamente desde ese árbol
   vendorizado.

Para transportar a la máquina air-gapged: copiar el repositorio completo
(incluido `Cargo.lock`), el directorio `vendor/` generado en la fase 1, y
activar su config de Cargo (`cp .cargo/config.toml.offline .cargo/config.toml`)
antes de correr la fase 2 allí. Verificado en esta sesión: las dos fases
corren de punta a punta contra este `Cargo.lock` (vendoriza, luego compila
`--offline --release` sin tocar la red).

**Lo que este script NO decide** (`docs/dsp-plan.md` §8.2 Fase 0, "CPU/SBC
target undecided"): arquitectura de destino (ARM vs x86), toolchain de
cross-compilación (`cross`/`cargo-zigbuild`), ni si el binario final es
`musl` estático o `glibc` dinámico — compila para el host actual tal cual.
Adaptar `--target`/el toolchain cuando Fase 0 cierre esa decisión.

## Unidad systemd

`packaging/lamula-dsp.service` arranca el binario como servicio systemd,
con reinicio automático (`Restart=on-failure`: un cierre limpio de DRx/RCP
no termina el proceso — reconecta solo, ver `crates/ingest`/
`crates/rcp-link` — así que si el proceso sí termina fue un fallo real de
socket, y reintentar tiene sentido) y endurecimiento básico
(`NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`) que
no cuesta nada en una instalación air-gapped de un solo operador
(`docs/dsp-plan.md` §3.2 excluye hardening/autenticación de alcance, pero
esto no es eso — es higiene de proceso gratuita).

Instalación (comandos también en la cabecera del propio fichero):

```sh
install -Dm755 lamula-dsp /opt/lamula-dsp/bin/lamula-dsp
install -Dm640 lamula-dsp.env /etc/lamula-dsp/lamula-dsp.env
install -Dm644 lamula-dsp.service /etc/systemd/system/lamula-dsp.service
useradd --system --no-create-home --shell /usr/sbin/nologin lamula-dsp
systemctl daemon-reload
systemctl enable --now lamula-dsp
```

**Sin `ReadWritePaths`**: este binario no escribe nada en disco hoy — el
archivo de I/Q crudo de investigación que el plan pone en alcance Stage 1
("Raw I/Q archive") no está implementado en `crates/service` todavía (ver
el doc-comment de `crate` en `crates/service/src/main.rs`, sección "Alcance
honesto"). Cuando exista, la unidad necesita esa directiva apuntando a su
directorio.

## Variables de entorno (`lamula-dsp.env`)

Ninguna variable de `crates/service::config::ServiceConfig` tiene valor por
defecto — el binario se niega a arrancar si falta cualquiera. La plantilla
completa, con el porqué de cada una, vive en
[`packaging/lamula-dsp.env.example`](https://github.com/vladimir1284/lamula-dsp/blob/main/packaging/lamula-dsp.env.example);
resumen:

| Variable | Qué es | Por qué no tiene default |
| --- | --- | --- |
| `LAMULA_DSP_DRX_ADDR` | Dirección del DRx a la que el DSP **conecta** (el DRx escucha, D-16 del proyecto DRx); el puerto es `TCP_PORT` del contrato, 9470. No es dirección de escucha: `0.0.0.0` no vale aquí | Direccionamiento de red, específico de instalación |
| `LAMULA_DSP_RCP_ADDR` | Dirección del RCP a la que el DSP **conecta** (regla "el productor conecta"); el puerto es el `--dsp-port` del gateway del RCP, 15551 por defecto. No es dirección de escucha: `0.0.0.0` no vale aquí | Direccionamiento de red, específico de instalación |
| `LAMULA_DSP_FULL_SCALE_COUNTS` | Cuenta ADC de amplitud unitaria | Convención de cuantización sin calibración real confirmada (`crates/ingest::wire`) |
| `LAMULA_DSP_SSI_COUNTS_PER_TURN` / `LAMULA_DSP_SSI_ZERO_OFFSET_DEG` | Resolución y cero del encoder SSI | Sin documentar en este repositorio (`crates/ingest::angle`) |
| `LAMULA_DSP_DRX_NCO_FS_HZ` / `LAMULA_DSP_DRX_NCO_WORD_BITS` | Reloj de referencia y anchura del acumulador de fase del NCO de recepción del DRx | Ni `DRx↔DSP` ni `DSP↔RCP` lo exponen — **bloquea comisionar el lazo de AFC contra hardware real** hasta confirmarlo con el proyecto DRx; la palabra es **absoluta** (15 MHz a 62,5 MHz y 32 bits = 1030792151) y el servicio envía `rx_if − freq` (ver `docs/algorithms/roadmap.md` "Decisiones cerradas" > "Lazo de AFC") |
| `LAMULA_DSP_FAST_TIME_DILATION` | Factor de dilatación temporal del banco (opcional, 1.0 por defecto; 4.0 para la ZedBoard frente a la ZU9) | Sólo aplica al banco de pruebas: escala el periodo de muestreo del burst que usa el lazo de AFC |
| `LAMULA_DSP_AFC_TAU_S` / `LAMULA_DSP_AFC_AMP_THRESHOLD` | Constante de tiempo y umbral de amplitud del lazo de AFC | Parámetros de instalación del propio lazo, no documentados en ningún contrato |

## Lo que sigue pendiente

- **Signo del NCO del AFC en la ZU9**: el lazo cierra sobre la ZedBoard con `NCO = rx_if − freq`
  (mezclador conjugado), pero no se ha medido en la ZU9. Si allí el signo es el contrario, el cambio
  es una línea en `crates/service/src/main.rs`. Procedimiento en `P-24` del proyecto DRx
  (`docs/alcance/pendientes.md`).
- **Instalador real** (paquete `.deb`/`.rpm`/imagen de SBC, no sólo los tres
  ficheros de `packaging/`): no existe todavía.
- **Cross-compilación ARM**: fuera de alcance hasta que Fase 0 decida la
  arquitectura de destino.
- **Archivo de I/Q crudo**: no wireado en `crates/service` — ver arriba.
- **Ops docs más allá de esta página**: procedimientos de arranque/parada
  operativos, troubleshooting de campo, no existen todavía.
