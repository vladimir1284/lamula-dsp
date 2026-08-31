// GENERADO por tools/gen_contract.py a partir de
// contract/schema/drx_dsp_v0_1.toml. NO EDITAR A MANO.
//
// Contrato DRx↔DSP v0.1 — lado DSP.
//
// Little-endian, empaquetado. Los `assert!` de tamaño viven en los tests
// del proyecto DSP; aquí van como constantes para que se puedan comprobar.

#![allow(dead_code)]

pub const MAGIC: u32 = 0x4C4D4452;
pub const VERSION_MAJOR: u8 = 0;
pub const VERSION_MINOR: u8 = 1;

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
    /// Bytes de carga útil detrás de la cabecera del mensaje.
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
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Config {
    /// Se devuelve tal cual en el config_ack.
    pub seq: u32,
    /// Divisor de PRF. PRF = FS_HZ/prf_div.
    pub prf_div: u32,
    /// Bins de rango pedidos.
    pub range_bins: u16,
    /// Índice en la tabla de anchos de pulso.
    pub pulse_width_idx: u8,
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
pub const CONFIG_SIZE: usize = 48;

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
    /// payload_len no cuadra con el mensaje.
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
