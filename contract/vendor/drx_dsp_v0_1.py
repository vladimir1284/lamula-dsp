"""GENERADO por tools/gen_contract.py a partir de contract/schema/drx_dsp_v0_1.toml. NO EDITAR A MANO.

Contrato DRx↔DSP v0.2 — referencia
de los tests de contrato. Es la tercera implementación generada de la misma
fuente: si esta y la de C no producen los mismos bytes, el codegen está mal.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass

MAGIC = 0x4C4D4452
VERSION_MAJOR = 0
VERSION_MINOR = 2

@dataclass
class Header:
    """Cabecera común a todo mensaje."""

    FORMAT = "<IBBBBI"
    SIZE = 12
    FIELDS = ("magic", "version_major", "version_minor", "msg_type", "flags", "payload_len",)

    magic: int = 0
    version_major: int = 0
    version_minor: int = 0
    msg_type: int = 0
    flags: int = 0
    payload_len: int = 0

    def pack(self) -> bytes:
        return struct.pack(self.FORMAT, *(getattr(self, name) for name in self.FIELDS))

    @classmethod
    def unpack(cls, data: bytes) -> "Header":
        return cls(*struct.unpack(cls.FORMAT, data[: cls.SIZE]))

class MsgType:
    """Tipos de mensaje."""

    RAY = 1
    STATUS = 2
    CONFIG = 3
    CONFIG_ACK = 4
    AFC = 5

@dataclass
class Ray:
    """Cabecera de un rayo. Detrás van `bins`·`n_channels` pares (I,Q) de int16"""

    FORMAT = "<IQIIIIHBBBBBB"
    SIZE = 36
    FIELDS = ("seq", "timestamp_ns", "trigger_count", "azimuth_raw", "elevation_raw", "prf_div", "bins", "pulse_width_idx", "pulse_mode", "cell_mode", "n_channels", "channel_mask", "ray_flags",)

    seq: int = 0
    timestamp_ns: int = 0
    trigger_count: int = 0
    azimuth_raw: int = 0
    elevation_raw: int = 0
    prf_div: int = 0
    bins: int = 0
    pulse_width_idx: int = 0
    pulse_mode: int = 0
    cell_mode: int = 0
    n_channels: int = 0
    channel_mask: int = 0
    ray_flags: int = 0

    def pack(self) -> bytes:
        return struct.pack(self.FORMAT, *(getattr(self, name) for name in self.FIELDS))

    @classmethod
    def unpack(cls, data: bytes) -> "Ray":
        return cls(*struct.unpack(cls.FORMAT, data[: cls.SIZE]))

@dataclass
class Status:
    """Status y BITE. Se emite periódicamente y ante cualquier cambio de estado."""

    FORMAT = "<IIIIIIBBH"
    SIZE = 28
    FIELDS = ("uptime_s", "bite_flags", "ssa_underruns", "dma_overruns", "ssi_errors", "ddc_overflows", "last_error", "pad0", "pad1",)

    uptime_s: int = 0
    bite_flags: int = 0
    ssa_underruns: int = 0
    dma_overruns: int = 0
    ssi_errors: int = 0
    ddc_overflows: int = 0
    last_error: int = 0
    pad0: int = 0
    pad1: int = 0

    def pack(self) -> bytes:
        return struct.pack(self.FORMAT, *(getattr(self, name) for name in self.FIELDS))

    @classmethod
    def unpack(cls, data: bytes) -> "Status":
        return cls(*struct.unpack(cls.FORMAT, data[: cls.SIZE]))

@dataclass
class Config:
    """Configuración completa. Se aplica de forma atómica: o entra entera o se"""

    FORMAT = "<IIHBBBBBBIIIIIIII"
    SIZE = 48
    FIELDS = ("seq", "prf_div", "range_bins", "pulse_width_idx", "pulse_mode", "cell_mode", "channel_mask", "scan_mode", "pad0", "trigger_delay_0", "trigger_delay_1", "trigger_delay_2", "trigger_delay_3", "trigger_width_0", "trigger_width_1", "trigger_width_2", "trigger_width_3",)

    seq: int = 0
    prf_div: int = 0
    range_bins: int = 0
    pulse_width_idx: int = 0
    pulse_mode: int = 0
    cell_mode: int = 0
    channel_mask: int = 0
    scan_mode: int = 0
    pad0: int = 0
    trigger_delay_0: int = 0
    trigger_delay_1: int = 0
    trigger_delay_2: int = 0
    trigger_delay_3: int = 0
    trigger_width_0: int = 0
    trigger_width_1: int = 0
    trigger_width_2: int = 0
    trigger_width_3: int = 0

    def pack(self) -> bytes:
        return struct.pack(self.FORMAT, *(getattr(self, name) for name in self.FIELDS))

    @classmethod
    def unpack(cls, data: bytes) -> "Config":
        return cls(*struct.unpack(cls.FORMAT, data[: cls.SIZE]))

@dataclass
class ConfigAck:
    """Respuesta a un config. `error` distinto de 0 significa que NO se aplicó nada."""

    FORMAT = "<IBBH"
    SIZE = 8
    FIELDS = ("seq", "error", "pad0", "pad1",)

    seq: int = 0
    error: int = 0
    pad0: int = 0
    pad1: int = 0

    def pack(self) -> bytes:
        return struct.pack(self.FORMAT, *(getattr(self, name) for name in self.FIELDS))

    @classmethod
    def unpack(cls, data: bytes) -> "ConfigAck":
        return cls(*struct.unpack(cls.FORMAT, data[: cls.SIZE]))

@dataclass
class Afc:
    """Corrección AFC: nueva palabra de fase del NCO, calculada por el DSP."""

    FORMAT = "<QII"
    SIZE = 16
    FIELDS = ("nco_phase_inc", "apply_at_seq", "pad0",)

    nco_phase_inc: int = 0
    apply_at_seq: int = 0
    pad0: int = 0

    def pack(self) -> bytes:
        return struct.pack(self.FORMAT, *(getattr(self, name) for name in self.FIELDS))

    @classmethod
    def unpack(cls, data: bytes) -> "Afc":
        return cls(*struct.unpack(cls.FORMAT, data[: cls.SIZE]))

class Error:
    """Códigos de rechazo del plano de control."""

    OK = 0
    PRF_RANGE_ILLEGAL = 1
    UNSUPPORTED_VERSION = 2
    UNKNOWN_MESSAGE = 3
    BAD_LENGTH = 4
    CELL_MODE_INVALID = 5
    PULSE_WIDTH_INVALID = 6
    CHANNEL_MASK_INVALID = 7
    SCAN_MODE_INVALID = 8
    NOT_CONFIGURED = 9

class RayFlag:
    """Banderas por rayo. Un rayo con problemas se MARCA, no se descarta."""

    AZEL_INVALID = 1
    DDC_OVERFLOW = 2
    TRUNCATED = 4
    FIRST_AFTER_CONFIG = 8
    TX_POL_V = 16

class BiteFlag:
    """Catálogo de fallos del plan de testing."""

    SSA_UNDERRUN = 1
    DMA_OVERRUN = 2
    SSI_TIMEOUT = 4
    SSI_FRAME_ERROR = 8
    MMCM_UNLOCKED = 16
    LINK_DOWN = 32
    CONFIG_REJECTED = 64
