// GENERADO por tools/gen_contract.py a partir de
// contract/schema/drx_dsp_v0_1.toml. NO EDITAR A MANO.
//
// Contrato DRx↔DSP v0.8 — lado DSP.
//
// Little-endian, empaquetado. Los `assert!` de tamaño viven en los tests
// del proyecto DSP; aquí van como constantes para que se puedan comprobar.

#![allow(dead_code)]

pub const MAGIC: u32 = 0x4C4D4452;
pub const VERSION_MAJOR: u8 = 0;
pub const VERSION_MINOR: u8 = 8;
pub const TCP_PORT: u16 = 9470;

/// Cabecera común a todo mensaje.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Header {
    /// 0x4C4D4452. Si no coincide, el flujo no es de este contrato.
    pub magic: u32,
    /// Incompatible al cambiar.
    pub version_major: u8,
    /// Compatible hacia atrás dentro del mismo major.
    pub version_minor: u8,
    /// Ver la tabla de tipos de mensaje.
    pub msg_type: u8,
    /// Reservado en v0.1; tiene que valer 0.
    pub flags: u8,
    /// Bytes que siguen a ESTA cabecera de 12 B: la cabecera del mensaje más su carga útil variable si la tiene. Un lector de tramas hace por tanto: leer 12 B, leer payload_len B, y ya tiene el mensaje entero sin conocer su tipo. Un ray de 1844 bins x 4 canales vale 36 + 1844x4x4 = 29540; un mensaje sin carga variable vale el tamaño de su struct (status 28, config 60, config_ack 8, afc 16, test_stimulus_select 8), nunca 0.
    pub payload_len: u32,
}
pub const HEADER_SIZE: usize = 12;

/// Tipos de mensaje.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgType {
    /// up
    Ray = 1,
    /// up
    Status = 2,
    /// down
    Config = 3,
    /// up
    ConfigAck = 4,
    /// down
    Afc = 5,
    /// down
    TestStimulusSelect = 6,
}

/// Cabecera de un rayo. Detrás van `bins`·`n_channels` pares (I,Q) de int16
/// entrelazados como I0 Q0 I1 Q1..., canal más rápido que bin.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ray {
    /// Contador de rayos, envuelve. Detecta pérdidas.
    pub seq: u32,
    /// Instante del trigger, reloj del DRx.
    pub timestamp_ns: u64,
    /// Disparos desde el arranque.
    pub trigger_count: u32,
    /// Cuenta cruda del encoder SSI de azimut.
    pub azimuth_raw: u32,
    /// Cuenta cruda del encoder SSI de elevación.
    pub elevation_raw: u32,
    /// Divisor de PRF vigente. PRF = FS_HZ/prf_div.
    pub prf_div: u32,
    /// Bins de rango en este rayo.
    pub bins: u16,
    /// Índice en la tabla de anchos de pulso.
    pub pulse_width_idx: u8,
    /// Modo de pulso vigente.
    pub pulse_mode: u8,
    /// 0 = celda fina, 1 = celda gruesa.
    pub cell_mode: u8,
    /// Canales presentes en la carga útil.
    pub n_channels: u8,
    /// Qué canales físicos son, bit por canal.
    pub channel_mask: u8,
    /// Ver la tabla de banderas de rayo.
    pub ray_flags: u8,
}
pub const RAY_SIZE: usize = 36;

/// Status y BITE. Se emite periódicamente y ante cualquier cambio de estado.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Status {
    /// Segundos desde el arranque del firmware.
    pub uptime_s: u32,
    /// Ver la tabla de banderas de BITE.
    pub bite_flags: u32,
    /// Underruns de la fuente de muestras.
    pub ssa_underruns: u32,
    /// Overruns del camino DMA.
    pub dma_overruns: u32,
    /// Timeouts y tramas malas de los encoders.
    pub ssi_errors: u32,
    /// Muestras saturadas a la salida del DDC.
    pub ddc_overflows: u32,
    /// Último código de error del plano de control.
    pub last_error: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u8,
    /// Relleno explícito; vale 0.
    pub pad1: u16,
}
pub const STATUS_SIZE: usize = 28;

/// Configuración completa. Se aplica de forma atómica: o entra entera o se
/// rechaza entera y el estado anterior se preserva.
/// 
/// Lleva DOS lotes de PRF/ancho de pulso por radial, `low` y `high`, porque eso es
/// lo que secuencia el motor de timing (D-13, Batch mode de NEXRAD) y lo que el
/// mapa de registros AXI-Lite expone desde v3. Hasta v0.5 este mensaje declaraba un
/// único par, que modelaba un supuesto que D-13 descartó: con aquel esquema el DSP
/// no tenía dónde pedir el segundo lote. Ver P-15.
/// 
/// Los dos lotes se validan por separado contra D-09 y la configuración entra o se
/// rechaza como un todo: un `config` cuyo lote `high` sea inválido NO deja aplicado
/// el lote `low`.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Config {
    /// Se devuelve tal cual en el config_ack.
    pub seq: u32,
    /// Divisor de PRF del lote bajo. PRF = FS_HZ/prf_div_low.
    pub prf_div_low: u32,
    /// Divisor de PRF del lote alto. PRF = FS_HZ/prf_div_high.
    pub prf_div_high: u32,
    /// Bins de rango del lote bajo (D-15).
    pub range_bins_low: u16,
    /// Bins de rango del lote alto (D-15).
    pub range_bins_high: u16,
    /// Pulsos del lote bajo dentro del radial.
    pub n_pulses_low: u16,
    /// Pulsos del lote alto dentro del radial.
    pub n_pulses_high: u16,
    /// Índice en la tabla de anchos de pulso, lote bajo.
    pub pulse_width_idx_low: u8,
    /// Índice en la tabla de anchos de pulso, lote alto.
    pub pulse_width_idx_high: u8,
    /// Modo de pulso.
    pub pulse_mode: u8,
    /// 0 = celda fina, 1 = celda gruesa.
    pub cell_mode: u8,
    /// Canales a capturar.
    pub channel_mask: u8,
    /// 0 = split cut, 1 = batch cut, 2 = doppler cut.
    pub scan_mode: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u8,
    /// Relleno explícito; vale 0.
    pub pad1: u8,
    /// Retardo del trigger 0, en ciclos de fs.
    pub trigger_delay_0: u32,
    /// Retardo del trigger 1, en ciclos de fs.
    pub trigger_delay_1: u32,
    /// Retardo del trigger 2, en ciclos de fs.
    pub trigger_delay_2: u32,
    /// Retardo del trigger 3, en ciclos de fs.
    pub trigger_delay_3: u32,
    /// Ancho del trigger 0, en ciclos de fs.
    pub trigger_width_0: u32,
    /// Ancho del trigger 1, en ciclos de fs.
    pub trigger_width_1: u32,
    /// Ancho del trigger 2, en ciclos de fs.
    pub trigger_width_2: u32,
    /// Ancho del trigger 3, en ciclos de fs.
    pub trigger_width_3: u32,
}
pub const CONFIG_SIZE: usize = 60;

/// Respuesta a un config. `error` distinto de 0 significa que NO se aplicó nada.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfigAck {
    /// El `seq` del config al que responde.
    pub seq: u32,
    /// Código de error; 0 es aceptado.
    pub error: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u8,
    /// Relleno explícito; vale 0.
    pub pad1: u16,
}
pub const CONFIG_ACK_SIZE: usize = 8;

/// Corrección AFC: nueva palabra de fase del NCO, calculada por el DSP.
/// 
/// Viaja como palabra de fase absoluta y no como offset en Hz a propósito: en Hz
/// haría falta que el DSP conociera `FS_HZ` del DRx, y eso rompería D-02 en cuanto
/// las dos plataformas divergieran.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Afc {
    /// Palabra de fase absoluta del NCO.
    pub nco_phase_inc: u64,
    /// Rayo a partir del cual aplicar; 0 = ya.
    pub apply_at_seq: u32,
    /// Relleno explícito; vale 0.
    pub pad0: u32,
}
pub const AFC_SIZE: usize = 16;

/// Elige qué vector de estímulo reproduce `vector_source` en la PL.
/// 
/// Existe porque en ZedBoard no hay ADC: el único origen de muestras es
/// `rtl/vector_source.sv`, y hasta v0.5 el vector que reproducía quedaba fijado en
/// tiempo de SÍNTESIS (`$readmemh` sobre la BRAM, `tools/gen_vector_source_init.py`).
/// Cambiar de estímulo costaba una corrida de Vivado entera, así que ninguna prueba
/// de punta a punta podía barrer el catálogo. Con este mensaje el par remoto elige
/// el vector en tiempo de ejecución y el firmware lo carga en la PL por AXI DMA
/// MM2S.
/// 
/// Es un mandato de BANCO DE PRUEBAS, no de operación: en la plataforma objetivo el
/// origen de muestras son los ADC reales y este mensaje no tiene efecto. El
/// receptor responde con `config_ack` reusando `seq`, igual que a un `config`.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TestStimulusSelect {
    /// Se devuelve tal cual en el config_ack.
    pub seq: u32,
    /// Índice en el catálogo; ver la enumeración stimulus_vector.
    pub vector_id: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u8,
    /// Veces que se reproduce el vector antes de parar. 0 = en bucle indefinido, que es el comportamiento que tenía el modo BRAM-loop.
    pub repeat: u16,
}
pub const TEST_STIMULUS_SELECT_SIZE: usize = 8;

/// Códigos de rechazo del plano de control.
pub mod error {
    /// Aceptado.
    pub const OK: u8 = 0;
    /// PRF y extensión de rango incompatibles (D-09).
    pub const PRF_RANGE_ILLEGAL: u8 = 1;
    /// version_major desconocido.
    pub const UNSUPPORTED_VERSION: u8 = 2;
    /// msg_type desconocido.
    pub const UNKNOWN_MESSAGE: u8 = 3;
    /// payload_len no cuadra: no coincide con el tamaño del struct del msg_type recibido más su carga útil variable si la tiene.
    pub const BAD_LENGTH: u8 = 4;
    /// cell_mode fuera de {0,1}.
    pub const CELL_MODE_INVALID: u8 = 5;
    /// Índice de ancho de pulso fuera de tabla.
    pub const PULSE_WIDTH_INVALID: u8 = 6;
    /// Máscara de canales vacía o fuera de rango.
    pub const CHANNEL_MASK_INVALID: u8 = 7;
    /// Modo de barrido desconocido.
    pub const SCAN_MODE_INVALID: u8 = 8;
    /// Llegó un mandato antes de la primera configuración.
    pub const NOT_CONFIGURED: u8 = 9;
    /// vector_id fuera del catálogo de stimulus_vector.
    pub const VECTOR_UNKNOWN: u8 = 10;
    /// msg_type conocido y bien formado, pero este receptor todavía no lo aplica. Distinto de unknown_message, que es para un msg_type que el receptor NO conoce: con ese el par remoto sabe que hablan versiones distintas, con este sabe que hablan la misma y que la función aún no está. Nada se aplicó.
    pub const UNIMPLEMENTED: u8 = 11;
    /// `nco_phase_inc` del mensaje `afc` no cabe en el acumulador de fase del NCO de este receptor. El campo viaja como u64 a proposito --la portadora ZU9 podria tener un acumulador mas ancho sin cambiar el esquema-- asi que un receptor con menos bits tiene que poder decirlo en vez de truncar en silencio, que convertiria una correccion de AFC equivocada en una frecuencia de mezcla equivocada sin ningun sintoma visible. El mensaje `afc` no lleva `seq` y no se contesta: este codigo viaja en `last_error` del mensaje `status`.
    pub const AFC_OUT_OF_RANGE: u8 = 12;
}

/// Bit por canal físico presente en channel_mask. El orden de channels[] en el payload de ray sigue el orden ascendente de los bits puestos en channel_mask; cada canal aporta los mismos `bins` que el resto del rayo. rx_0..rx_3 son 2 conversores por polarización (H, V): uno a ganancia nominal y otro con la señal atenuada, para extender el rango dinámico (confirmado en sesión, 8-sep-2026). El emparejamiento H/V de arriba está confirmado; el orden concreto rx_0 vs rx_1 (¿cuál es el nominal y cuál el atenuado de cada polarización?) es una CONVENCIÓN asumida aquí, sin confirmar todavía contra el cableado físico — ver docs/alcance/pendientes.md, A10.
pub mod channel {
    /// H, ganancia nominal (convención asumida, ver doc del enum).
    pub const RX_0: u8 = 1;
    /// H, señal atenuada — rango dinámico extendido (convención asumida, ver doc del enum).
    pub const RX_1: u8 = 2;
    /// V, ganancia nominal (convención asumida, ver doc del enum).
    pub const RX_2: u8 = 4;
    /// V, señal atenuada — rango dinámico extendido (convención asumida, ver doc del enum).
    pub const RX_3: u8 = 8;
    /// Muestra del transmisor. Un solo conversor físico (n_tx_burst_ch=1, 8-sep-2026): trae energía sólo durante la ventana del pulso transmitido; el resto de sus bins es ruido/silencio.
    pub const TX_BURST_0: u8 = 16;
}

/// Banderas por rayo. Un rayo con problemas se MARCA, no se descarta.
pub mod ray_flag {
    /// Lectura de encoder inválida en este rayo.
    pub const AZEL_INVALID: u8 = 1;
    /// Hubo saturación en el DDC dentro del rayo.
    pub const DDC_OVERFLOW: u8 = 2;
    /// El rayo salió corto.
    pub const TRUNCATED: u8 = 4;
    /// Primer rayo con la configuración nueva.
    pub const FIRST_AFTER_CONFIG: u8 = 8;
    /// Este pulso se transmitió en V; a cero, se transmitió en H. Sólo tiene sentido con polarización alternante H/V; en canal único o simultánea (STAR) el bit no se usa y vale 0.
    pub const TX_POL_V: u8 = 16;
    /// Este rayo lo produjo el lote ALTO del radial; a cero, el lote bajo (D-13). Es una bandera y no un campo porque prf_div, bins y pulse_width_idx de la cabecera ya llevan los valores VIGENTES para ese rayo: el bit sólo dice de cuál de los dos lotes salieron, que es lo que hace falta para agrupar rayos por lote sin reconstruir el radial.
    pub const BATCH_HIGH: u8 = 32;
}

/// Catálogo de fallos del plan de testing.
pub mod bite_flag {
    /// Underrun de la fuente de muestras.
    pub const SSA_UNDERRUN: u32 = 1;
    /// Overrun del camino DMA.
    pub const DMA_OVERRUN: u32 = 2;
    /// Timeout de encoder SSI.
    pub const SSI_TIMEOUT: u32 = 4;
    /// Trama SSI corta, larga o salto de Gray.
    pub const SSI_FRAME_ERROR: u32 = 8;
    /// Pérdida de lock del MMCM.
    pub const MMCM_UNLOCKED: u32 = 16;
    /// Enlace Ethernet caído.
    pub const LINK_DOWN: u32 = 32;
    /// Se rechazó una configuración.
    pub const CONFIG_REJECTED: u32 = 64;
}

/// Catálogo v1 de vectores de estímulo, los ocho casos de tools/gen_l1_vectors.py — la misma lista contra la que L1 compara la IP real de Xilinx con el modelo dorado bajo D-12. El catálogo puede CRECER dentro del mismo version_major (añadir un valor es compatible hacia atrás); reordenar o quitar valores no. En la plataforma objetivo, con ADC reales, esta enumeración no tiene efecto.
pub mod stimulus_vector {
    /// IF nominal: lo que el NCO tiene que llevar a continua exactamente.
    pub const TONO_IF: u8 = 0;
    /// Desplazado +0,001 dentro de la banda de paso: ejercita el FIR fuera de continua.
    pub const TONO_DESPLAZADO: u8 = 1;
    /// Rechazo de imagen e intermodulación del mezclador complejo.
    pub const DOS_TONOS: u8 = 2;
    /// Simétrico del anterior al otro lado del IF; cuarto estímulo distinto para los canales del quad.
    pub const TONO_DESPLAZADO_NEG: u8 = 3;
    /// Degradación D9: ruido a 10 dB de SNR.
    pub const TONO_CON_RUIDO: u8 = 4;
    /// Degradación D9: offset de continua a -12 dBFS.
    pub const TONO_CON_OFFSET_DC: u8 = 5;
    /// Degradación D9: desbalance I/Q de 1 dB y 3 grados.
    pub const TONO_DESBALANCEADO: u8 = 6;
    /// Entrada a +6 dBFS: el ADC satura, no envuelve. Ejercita el reporte de overflow.
    pub const TONO_SOBRE_ESCALA: u8 = 7;
}
