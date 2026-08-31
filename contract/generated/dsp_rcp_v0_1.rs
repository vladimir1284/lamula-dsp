// GENERADO por tools/gen_contract.py a partir de
// contract/schema/dsp_rcp_v0_1.toml. NO EDITAR A MANO.
//
// Contrato DSP↔RCP v0.1 — lado DSP.
//
// Little-endian, empaquetado. Los asertos de tamaño y desplazamiento
// viven en `contract/tests/dsp_rcp_layout.rs`; aquí van las constantes
// de tamaño para que se puedan comprobar contra `size_of`.

#![allow(dead_code)]

pub const MAGIC: u32 = 0x4C4D4453;
pub const VERSION_MAJOR: u8 = 0;
pub const VERSION_MINOR: u8 = 1;

/// Cabecera común a todo mensaje.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Header {
    /// 0x4C4D4453. Si no coincide, el flujo no es de este contrato.
    pub magic: u32,
    /// Incompatible al cambiar.
    pub version_major: u8,
    /// Compatible hacia atrás dentro del mismo major.
    pub version_minor: u8,
    /// Ver la tabla de tipos de mensaje.
    pub msg_type: u8,
    /// Reservado en v0.1; tiene que valer 0.
    pub flags: u8,
    /// Bytes que siguen a ESTA cabecera de 12 B, contando la cabecera del mensaje
    /// más su carga útil variable si la tiene. Un lector de tramas hace por tanto: leer
    /// 12 B, leer payload_len B, y ya tiene el mensaje entero sin conocer su tipo. Para
    /// un moment_ray de 4 celdas y 2 momentos vale 88 + 2·(16 + 4·4) = 152, no 64.
    pub payload_len: u32,
}
pub const HEADER_SIZE: usize = 12;

/// Tipos de mensaje que viajan sueltos por el cable.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgType {
    /// up
    MomentRay = 1,
    /// up
    SpectrumFrame = 2,
    /// up
    Status = 3,
    /// up
    BiteEvent = 4,
    /// up
    ConfigAck = 5,
    /// up
    SelftestResult = 6,
    /// up
    Capabilities = 7,
    /// down
    Config = 8,
    /// down
    Control = 9,
    /// down
    SelftestRequest = 10,
}

/// Un radial de momentos: la observación autoritativa que el RCP archiva
/// como Level-II y sirve a ORPG.
///
/// Detrás de esta cabecera van `n_moments` bloques, cada uno formado por un
/// descriptor `moment_field` de 16 B seguido de `n_gates` valores. El tipo de los
/// valores lo dice `moment_field.data_type`; en v0.1 siempre es f32.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MomentRay {
    /// Contador de radiales, envuelve. Detecta pérdidas.
    pub seq: u32,
    /// Instante del primer pulso del radial en hora de pared, ns desde el epoch UTC. Es el que se archiva en Level-II y se sirve a ORPG.
    pub acq_time_utc_ns: u64,
    /// El mismo instante en el reloj monótono del DSP, ns. Sirve para ordenar y medir intervalos sin que un salto de UTC los corrompa; NO comparable entre procesos.
    pub acq_monotonic_ns: u64,
    /// Volumen al que pertenece. Enmarca el archivo Level-II.
    pub volume_seq: u32,
    /// Barrido dentro del volumen.
    pub sweep_seq: u16,
    /// Radial dentro del barrido.
    pub ray_index: u16,
    /// Celdas de rango por momento.
    pub n_gates: u16,
    /// Pulsos integrados en este radial.
    pub n_pulses: u16,
    /// Celdas con adquisición correcta. Distinto de que el enlace esté vivo.
    pub bins_valid: u16,
    /// Bloques de momento en la carga útil.
    pub n_moments: u8,
    /// Ver la enumeración de modos de barrido.
    pub sweep_mode: u8,
    /// Ver la enumeración de modos de dealiasing.
    pub prf_mode: u8,
    /// Ver la tabla de banderas de radial.
    pub ray_flags: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u16,
    /// Azimut al abrir el radial, grados.
    pub az_start_deg: f32,
    /// Azimut al cerrarlo. Con az_start da el ancho barrido.
    pub az_end_deg: f32,
    /// Elevación al abrir el radial, grados.
    pub el_start_deg: f32,
    /// Elevación al cerrarlo, grados.
    pub el_end_deg: f32,
    /// Ángulo nominal del barrido: elevación en PPI, azimut en RHI.
    pub fixed_angle_deg: f32,
    /// Rango al centro de la primera celda, metros.
    pub start_range_m: f32,
    /// Separación entre centros de celda, metros.
    pub gate_spacing_m: f32,
    /// PRF efectiva del radial. En dual-PRF, la media.
    pub prf_hz: f32,
    /// Velocidad no ambigua tras dealiasing, m/s.
    pub nyquist_velocity: f32,
    /// Rango no ambiguo, metros. Es c/(2·PRF) salvo recuperación de trip.
    pub unambiguous_range_m: f32,
    /// Suelo de ruido vigente al procesar, dBm.
    pub noise_floor_dbm: f32,
    /// Constante de radar aplicada, dB. El RCP la necesita para rehacer dBZ.
    pub radar_constant_db: f32,
}
pub const MOMENT_RAY_SIZE: usize = 88;

/// Descriptor de un momento dentro de la carga útil de un moment_ray.
///
/// No viaja suelto: siempre va incrustado, y por eso su type_id es 0. Detrás de
/// cada descriptor van `n_gates` valores del tipo que indica `data_type`.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MomentField {
    /// Qué momento es. Ver la enumeración de momentos.
    pub kind: u8,
    /// Codificación de los valores. En v0.1 siempre f32.
    pub data_type: u8,
    /// Ver la tabla de banderas de momento.
    pub flags: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u8,
    /// Valores que siguen. Tiene que coincidir con el n_gates del radial.
    pub n_gates: u32,
    /// Factor de escala. Vale 1.0 con data_type f32.
    pub scale: f32,
    /// Desplazamiento. Vale 0.0 con data_type f32.
    pub offset: f32,
}
pub const MOMENT_FIELD_SIZE: usize = 16;

/// Traza del analizador de espectro de FI. Detrás van `n_bins` valores f32
/// en dB, de menor a mayor frecuencia.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpectrumFrame {
    /// Contador de tramas, envuelve.
    pub seq: u32,
    /// Instante de la captura en hora de pared, ns desde el epoch UTC.
    pub capture_time_utc_ns: u64,
    /// Puntos de la traza.
    pub n_bins: u16,
    /// Canal de recepción al que corresponde.
    pub channel: u8,
    /// Reservado en v0.1; vale 0.
    pub flags: u8,
    /// Frecuencia central de la traza, Hz.
    pub center_freq_hz: f32,
    /// Anchura total barrida, Hz.
    pub span_hz: f32,
    /// Nivel de referencia, dBm.
    pub ref_level_dbm: f32,
    /// Relleno explícito; vale 0.
    pub pad0: u32,
}
pub const SPECTRUM_FRAME_SIZE: usize = 32;

/// Salud y telemetría. Se emite periódicamente y ante cualquier cambio de
/// estado.
///
/// Deliberadamente no colapsa en un bit de vivo/muerto: lleva completitud de datos
/// (bins_ok frente a bins_total), deriva del periodo de disparo (medido frente a
/// mandado) y lectura de suelo de ruido y offset de continua por canal, que son las
/// tres cosas que el plan (§6.1) exige poder vigilar por separado.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Status {
    /// Segundos desde el arranque del servicio.
    pub uptime_s: u32,
    /// Fase vigente: configuración o marcha. Ver la enumeración.
    pub phase: u8,
    /// Severidad agregada. Ver la enumeración.
    pub severity: u8,
    /// Último código de error del plano de control.
    pub last_error: u8,
    /// Canales de recepción con lectura válida en este mensaje.
    pub n_rx_channels: u8,
    /// Modos de proceso disponibles ahora mismo.
    pub capability_flags: u32,
    /// Ver la tabla de banderas de BITE.
    pub bite_flags: u32,
    /// `seq` de la configuración vigente. Permite confirmar qué se aplicó.
    pub config_seq: u32,
    /// Radiales recibidos del DRx.
    pub rays_in: u32,
    /// Radiales de momentos emitidos al RCP.
    pub rays_out: u32,
    /// Radiales descartados por contrapresión o trama mala.
    pub rays_dropped: u32,
    /// Ocupación de la cola de ingesta, en radiales.
    pub queue_depth: u32,
    /// Celdas adquiridas correctamente desde el último reset.
    pub bins_ok: u32,
    /// Celdas esperadas en el mismo intervalo.
    pub bins_total: u32,
    /// Periodo de disparo mandado, ns.
    pub trigger_period_cmd_ns: u32,
    /// Periodo de disparo medido, ns. La diferencia es la deriva.
    pub trigger_period_meas_ns: u32,
    /// Relleno explícito; vale 0.
    pub pad0: u32,
    /// Suelo de ruido del canal 0, dBm.
    pub noise_floor_dbm_0: f32,
    /// Suelo de ruido del canal 1, dBm.
    pub noise_floor_dbm_1: f32,
    /// Suelo de ruido del canal 2, dBm.
    pub noise_floor_dbm_2: f32,
    /// Suelo de ruido del canal 3, dBm.
    pub noise_floor_dbm_3: f32,
    /// Offset de continua en I, canal 0.
    pub dc_offset_i_0: f32,
    /// Offset de continua en I, canal 1.
    pub dc_offset_i_1: f32,
    /// Offset de continua en I, canal 2.
    pub dc_offset_i_2: f32,
    /// Offset de continua en I, canal 3.
    pub dc_offset_i_3: f32,
    /// Offset de continua en Q, canal 0.
    pub dc_offset_q_0: f32,
    /// Offset de continua en Q, canal 1.
    pub dc_offset_q_1: f32,
    /// Offset de continua en Q, canal 2.
    pub dc_offset_q_2: f32,
    /// Offset de continua en Q, canal 3.
    pub dc_offset_q_3: f32,
}
pub const STATUS_SIZE: usize = 104;

/// Un suceso de BITE con su instante. Detrás van `text_len` bytes UTF-8 de
/// texto libre para diagnóstico; el código es lo que se filtra y se historia, el
/// texto es para el operador.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BiteEvent {
    /// Instante del suceso en hora de pared, ns desde el epoch UTC. Lo lee un operador, así que nunca es monótono.
    pub event_time_utc_ns: u64,
    /// Código del catálogo de fallos.
    pub code: u32,
    /// Valor asociado; su sentido depende del código.
    pub value: u32,
    /// Ver la enumeración de severidad.
    pub severity: u8,
    /// Componente del pipeline que lo emite.
    pub subsystem: u8,
    /// Bytes UTF-8 de texto detrás de la cabecera.
    pub text_len: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u8,
}
pub const BITE_EVENT_SIZE: usize = 20;

/// Respuesta a un config. `error` distinto de 0 significa que NO se aplicó
/// nada y que la configuración anterior sigue vigente.
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

/// Resultado del autotest de enlace. El plan (§6.1) lo exige en cada
/// reconexión del RCP: un apretón de manos TCP no basta para fiarse del enlace
/// para control.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelftestResult {
    /// El `seq` de la petición a la que responde.
    pub seq: u32,
    /// El nonce de la petición, devuelto tal cual.
    pub nonce: u32,
    /// Modos de proceso disponibles.
    pub capability_flags: u32,
    /// Código de error; 0 es enlace apto para control.
    pub error: u8,
    /// Versión de contrato que habla el DSP.
    pub version_major: u8,
    /// Versión de contrato que habla el DSP.
    pub version_minor: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u8,
}
pub const SELFTEST_RESULT_SIZE: usize = 16;

/// Qué sabe hacer esta compilación del DSP. Se responde a un control con
/// mandato `request_capabilities`, y es lo que permite al RCP no ofrecer al
/// operador un modo que el procesador no implementa.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// Momentos que este DSP puede producir, un bit por momento.
    pub moment_mask: u32,
    /// Modos de dealiasing disponibles, un bit por modo.
    pub dealias_mask: u32,
    /// Estimadores disponibles, un bit por estimador.
    pub estimator_mask: u32,
    /// Celdas de rango máximas por radial.
    pub max_gates: u32,
    /// Pulsos máximos integrables por radial.
    pub max_pulses: u16,
    /// Canales de recepción que procesa.
    pub n_rx_channels: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u8,
}
pub const CAPABILITIES_SIZE: usize = 20;

/// Configuración completa. Se aplica de forma atómica: o entra entera o se
/// rechaza entera y el estado anterior se preserva.
///
/// Sólo se acepta en fase de configuración. En marcha se rechaza con
/// `not_in_setup_phase`: el plan (§6.1) exige que aplicar configuración y arrancar
/// la adquisición sean pasos distintos, y no que la configuración se cuele a mitad
/// del flujo.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Config {
    /// Se devuelve tal cual en el config_ack.
    pub seq: u32,
    /// Momentos a emitir, un bit por momento.
    pub moment_mask: u32,
    /// Pulsos a integrar por radial.
    pub n_pulses: u16,
    /// Celdas de rango por radial.
    pub n_gates: u16,
    /// Filtro de clutter. Ver la enumeración.
    pub clutter_filter: u8,
    /// Modo de dealiasing de velocidad. Ver la enumeración.
    pub dealias_mode: u8,
    /// Modo de barrido. Ver la enumeración.
    pub sweep_mode: u8,
    /// Estimador de momentos. Ver la enumeración.
    pub estimator: u8,
    /// Filtrado de interferencia de banda estrecha: 0 no, 1 sí.
    pub rfi_filter: u8,
    /// Recuperación de trip múltiple: 0 no, 1 sí.
    pub range_dealias: u8,
    /// Numerador de la razón dual-PRF; 0 si no aplica.
    pub prf_ratio_num: u8,
    /// Denominador de la razón dual-PRF; 0 si no aplica.
    pub prf_ratio_den: u8,
    /// Rango de la primera celda, metros.
    pub start_range_m: f32,
    /// Separación entre celdas, metros. Fija el tamaño de celda.
    pub gate_spacing_m: f32,
    /// PRF pedida, Hz. Se valida contra la extensión de rango.
    pub prf_hz: f32,
    /// Umbral de SQI por debajo del cual se censura la celda.
    pub sqi_threshold: f32,
    /// Umbral de señal sobre ruido, dB.
    pub sig_threshold: f32,
    /// Umbral de corrección de clutter, dB.
    pub ccor_threshold: f32,
    /// Umbral logarítmico de potencia, dB.
    pub log_threshold: f32,
    /// Anchura espectral asumida del clutter, m/s.
    pub clutter_width_ms: f32,
    /// Constante de radar, dB.
    pub radar_constant_db: f32,
    /// Suelo de ruido de referencia, dBm.
    pub noise_floor_dbm: f32,
    /// Ganancia del receptor, dB.
    pub receiver_gain_db: f32,
    /// Corrección de sesgo de ZDR, dB.
    pub zdr_offset_db: f32,
    /// Fase diferencial del sistema a restar, grados.
    pub phidp_offset_deg: f32,
    /// Longitud de onda, metros. Escala la velocidad.
    pub wavelength_m: f32,
    /// Relleno explícito; vale 0.
    pub pad0: u32,
}
pub const CONFIG_SIZE: usize = 80;

/// Mandato del plano de control. Se responde siempre con un config_ack.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Control {
    /// Se devuelve tal cual en el config_ack.
    pub seq: u32,
    /// Ver la enumeración de mandatos.
    pub command: u8,
    /// Relleno explícito; vale 0.
    pub pad0: u8,
    /// Relleno explícito; vale 0.
    pub pad1: u16,
}
pub const CONTROL_SIZE: usize = 8;

/// Arranca el autotest de enlace. Obligatorio en cada reconexión del RCP.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelftestRequest {
    /// Se devuelve tal cual en el selftest_result.
    pub seq: u32,
    /// Valor arbitrario que el DSP devuelve, para casar respuesta con petición.
    pub nonce: u32,
}
pub const SELFTEST_REQUEST_SIZE: usize = 8;

/// Códigos de rechazo del plano de control.
pub mod error {
    /// Aceptado.
    pub const OK: u8 = 0;
    /// version_major desconocido.
    pub const UNSUPPORTED_VERSION: u8 = 1;
    /// msg_type desconocido.
    pub const UNKNOWN_MESSAGE: u8 = 2;
    /// payload_len no cuadra con el mensaje.
    pub const BAD_LENGTH: u8 = 3;
    /// Llegó un config estando en marcha.
    pub const NOT_IN_SETUP_PHASE: u8 = 4;
    /// Llegó un arranque antes de la primera configuración.
    pub const NOT_CONFIGURED: u8 = 5;
    /// Se pidió un momento que esta compilación no produce.
    pub const MOMENT_UNSUPPORTED: u8 = 6;
    /// Se pidió un modo de dealiasing no disponible.
    pub const DEALIAS_UNSUPPORTED: u8 = 7;
    /// Se pidió un estimador no disponible.
    pub const ESTIMATOR_UNSUPPORTED: u8 = 8;
    /// Un umbral cae fuera de su rango admisible.
    pub const THRESHOLD_OUT_OF_RANGE: u8 = 9;
    /// PRF y extensión de rango incompatibles; ver D-09 del DRx.
    pub const PRF_RANGE_ILLEGAL: u8 = 10;
    /// n_gates por encima de max_gates.
    pub const GATE_COUNT_ILLEGAL: u8 = 11;
    /// El autotest de enlace no pasó.
    pub const SELFTEST_FAILED: u8 = 12;
    /// No hay enlace con el DRx; no se puede arrancar.
    pub const DRX_LINK_DOWN: u8 = 13;
}

/// Vocabulario canónico de momentos, común a los planes del DSP y del RCP.
/// Reconcilia el nombrado heredado de Vesta (dBZ/dBT) con el del RCP (UZ/CZ).
pub mod moment_kind {
    /// Reflectividad sin corregir, dBZ.
    pub const UZ: u8 = 0;
    /// Reflectividad corregida, dBZ.
    pub const CZ: u8 = 1;
    /// Velocidad radial media, m/s.
    pub const V: u8 = 2;
    /// Ancho espectral, m/s.
    pub const W: u8 = 3;
    /// Reflectividad diferencial, dB.
    pub const ZDR: u8 = 4;
    /// Fase diferencial, grados.
    pub const PHIDP: u8 = 5;
    /// Fase diferencial específica, grados/km.
    pub const KDP: u8 = 6;
    /// Razón de despolarización lineal, dB.
    pub const LDR: u8 = 7;
    /// Coeficiente de correlación copolar, adimensional.
    pub const RHOHV: u8 = 8;
    /// Índice de calidad de señal, 0 a 1.
    pub const SQI: u8 = 9;
    /// Corrección de clutter aplicada, dB.
    pub const CCOR: u8 = 10;
    /// Señal sobre ruido, dB.
    pub const SIG: u8 = 11;
    /// Componente en fase cruda.
    pub const I: u8 = 12;
    /// Componente en cuadratura cruda.
    pub const Q: u8 = 13;
}

/// Banderas por radial. Un radial con problemas se MARCA, no se descarta.
pub mod ray_flag {
    /// Primer radial del barrido.
    pub const SWEEP_START: u8 = 1;
    /// Último radial del barrido.
    pub const SWEEP_END: u8 = 2;
    /// Primer radial del volumen.
    pub const VOLUME_START: u8 = 4;
    /// Último radial del volumen. Cierra el fichero Level-II.
    pub const VOLUME_END: u8 = 8;
    /// Alguna celda quedó censurada por umbral.
    pub const CENSORED: u8 = 16;
    /// El dealiasing no convergió en este radial.
    pub const DEALIAS_FAILED: u8 = 32;
    /// Se aplicó filtrado de clutter.
    pub const CLUTTER_FILTERED: u8 = 64;
    /// Primer radial con la configuración nueva.
    pub const FIRST_AFTER_CONFIG: u8 = 128;
}

/// Banderas por bloque de momento dentro de un radial.
pub mod moment_flag {
    /// El bloque contiene celdas sin dato, codificadas como NaN.
    pub const HAS_MISSING: u8 = 1;
    /// El momento lleva correcciones de calibración aplicadas.
    pub const CORRECTED: u8 = 2;
    /// El momento se calculó tras el filtro de clutter.
    pub const FILTERED: u8 = 4;
}

/// Fases del DSP. Configurar y adquirir son pasos distintos.
pub mod phase {
    /// Admite configuración; no emite momentos.
    pub const SETUP: u8 = 0;
    /// Emite momentos; rechaza configuración.
    pub const RUNNING: u8 = 1;
    /// Parado por fallo; sólo admite status y autotest.
    pub const FAULT: u8 = 2;
}

/// Mandatos del plano de control.
pub mod command {
    /// Para la adquisición y vuelve a fase de configuración.
    pub const ENTER_SETUP: u8 = 0;
    /// Pasa a marcha con la configuración vigente.
    pub const START: u8 = 1;
    /// Para la adquisición sin perder la configuración.
    pub const STOP: u8 = 2;
    /// Pide un status inmediato.
    pub const REQUEST_STATUS: u8 = 3;
    /// Pide de vuelta la configuración vigente.
    pub const REQUEST_CONFIG: u8 = 4;
    /// Pide el mensaje de capacidades.
    pub const REQUEST_CAPABILITIES: u8 = 5;
    /// Pone a cero los contadores de telemetría.
    pub const RESET_COUNTERS: u8 = 6;
}

/// Modos de barrido.
pub mod sweep_mode {
    /// Azimut variable a elevación fija.
    pub const PPI: u8 = 0;
    /// Elevación variable a azimut fijo.
    pub const RHI: u8 = 1;
    /// Sector de azimut acotado.
    pub const SECTOR: u8 = 2;
    /// Antena parada en una posición.
    pub const POINT: u8 = 3;
    /// Movimiento gobernado por el operador.
    pub const MANUAL: u8 = 4;
}

/// Modos de extensión del intervalo de velocidad no ambigua.
pub mod dealias_mode {
    /// PRF único; Nyquist sin extender.
    pub const NONE: u8 = 0;
    /// PRF alternante por radial.
    pub const DUAL_PRF: u8 = 1;
    /// Periodo escalonado dentro del radial.
    pub const STAGGERED_PRT: u8 = 2;
}

/// Estimadores de momentos.
pub mod estimator {
    /// Autocovarianza a retardo 1. Primario.
    pub const PULSE_PAIR: u8 = 0;
    /// FFT y ajuste espectral. Alternativo, más caro.
    pub const SPECTRAL: u8 = 1;
}

/// Filtros de eco fijo.
pub mod clutter_filter {
    /// Sin filtrar.
    pub const NONE: u8 = 0;
    /// GMAP: ajuste gaussiano e interpolación del hueco.
    pub const GMAP: u8 = 1;
    /// Notch fijo en velocidad cero.
    pub const NOTCH: u8 = 2;
}

/// Codificación de los valores de un bloque de momento.
pub mod data_type {
    /// IEEE-754 de 32 bits. Único tipo en v0.1.
    pub const F32: u8 = 0;
    /// Reservado: entero de 16 bits con scale y offset.
    pub const I16_SCALED: u8 = 1;
}

/// Niveles de severidad, comunes a status y a los sucesos de BITE.
pub mod severity {
    /// Informativo; no degrada el servicio.
    pub const INFO: u8 = 0;
    /// Degradación que no impide operar.
    pub const WARNING: u8 = 1;
    /// Fallo que impide producir momentos válidos.
    pub const FAULT: u8 = 2;
    /// La configuración vigente es inconsistente.
    pub const CONFIG_ERROR: u8 = 3;
}

/// Modos de proceso que una compilación del DSP puede ofrecer.
pub mod capability_flag {
    /// Estimadores polarimétricos disponibles.
    pub const DUAL_POL: u32 = 1;
    /// Estimador espectral disponible.
    pub const SPECTRAL_ESTIMATOR: u32 = 2;
    /// Dealiasing dual-PRF disponible.
    pub const DUAL_PRF: u32 = 4;
    /// Dealiasing por PRT escalonado disponible.
    pub const STAGGERED_PRT: u32 = 8;
    /// Recuperación de trip múltiple disponible.
    pub const RANGE_DEALIAS: u32 = 16;
    /// Filtrado de interferencia de banda estrecha disponible.
    pub const RFI_FILTER: u32 = 32;
    /// Analizador de espectro de FI disponible.
    pub const SPECTRUM_FEED: u32 = 64;
    /// Volcado de series temporales crudas disponible.
    pub const IQ_ARCHIVE: u32 = 128;
}

/// Catálogo de fallos del DSP.
pub mod bite_flag {
    /// Se perdieron radiales del DRx.
    pub const INGEST_DROP: u32 = 1;
    /// La cola de ingesta se desbordó.
    pub const QUEUE_OVERFLOW: u32 = 2;
    /// Enlace con el DRx caído.
    pub const DRX_LINK_DOWN: u32 = 4;
    /// El DRx rechazó una configuración.
    pub const DRX_CONFIG_REJECTED: u32 = 8;
    /// El periodo de disparo medido se apartó del mandado.
    pub const TRIGGER_DRIFT: u32 = 16;
    /// El suelo de ruido se apartó del calibrado.
    pub const NOISE_FLOOR_DRIFT: u32 = 32;
    /// La estimación de momentos no siguió el ritmo de radiales.
    pub const MOMENT_OVERRUN: u32 = 64;
    /// La calibración lleva demasiado sin verificarse.
    pub const CALIBRATION_STALE: u32 = 128;
    /// Enlace con el RCP caído.
    pub const RCP_LINK_DOWN: u32 = 256;
    /// Sin espacio para el archivo de I/Q crudo.
    pub const ARCHIVE_FULL: u32 = 512;
}
