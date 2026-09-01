//! Máquina de estados `setup`/`running`/`fault` del plano de control
//! (`docs/dsp-plan.md` §6.1; enumeración `phase` del esquema).
//!
//! Cubre sólo lo que el contrato documenta explícitamente:
//!
//! - `config` sólo se acepta en `setup`; en cualquier otra fase se rechaza
//!   con [`error::NOT_IN_SETUP_PHASE`] (doc del mensaje `config`: "Sólo se
//!   acepta en fase de configuración").
//! - `start` exige haber aplicado un `config` antes; si no, se rechaza con
//!   [`error::NOT_CONFIGURED`].
//! - `enter_setup`/`stop` vuelven a `setup` sin borrar la configuración
//!   vigente (el mensaje `config` no se re-envía tras un `stop`).
//! - `request_status`/`request_config`/`request_capabilities`/
//!   `reset_counters` son de sólo lectura/telemetría: se aceptan en
//!   cualquier fase, no cambian estado.
//!
//! Deliberadamente NO cubre (huecos reales del contrato o de este repo, no
//! omisiones descuidadas):
//!
//! - `error::DRX_LINK_DOWN`: si `start` debe rechazarse por enlace DRx caído
//!   es una pregunta que sólo puede responder quien tenga ese enlace
//!   (`lamula_ingest`), no este módulo. Quien llame a
//!   [`Session::handle_command`] debe comprobarlo antes de llamar, o tratar
//!   el `Ok(())` de un `start` como condicional a esa comprobación externa.
//! - Cuándo entrar en `fault`: eso es política del Status & BITE Manager
//!   (fuera de alcance de este crate, ver `crate` doc). [`Session::enter_fault`]
//!   sólo hace la transición mecánica; decidir cuándo llamarla no es de este
//!   módulo.
//! - Un `command` fuera de la tabla de la enumeración `command` (valores
//!   distintos de 0..=6): el esquema no define qué código de rechazo le
//!   corresponde. Se devuelve [`error::UNKNOWN_MESSAGE`] como aproximación
//!   razonable (mismo significado: "no reconozco lo que me pediste"), no
//!   porque el contrato lo diga literalmente para este campo.

use lamula_contract::dsp_rcp::{command, error, phase, Capabilities, Config};

use crate::validate::validate_config;

/// Estado del plano de control de un enlace `DSP↔RCP`. Función pura salvo
/// por su propio estado interno: no toca red ni sabe de `wire`/`tcp`; quien
/// decodifica los mensajes `down` llama a sus métodos y usa el `Result` para
/// rellenar el `error` de `config_ack`.
pub struct Session {
    phase: u8,
    config: Option<Config>,
    caps: Capabilities,
}

impl Session {
    /// Arranca en `setup`, sin configuración aplicada.
    pub fn new(caps: Capabilities) -> Self {
        Session {
            phase: phase::SETUP,
            config: None,
            caps,
        }
    }

    pub fn phase(&self) -> u8 {
        self.phase
    }

    pub fn config(&self) -> Option<&Config> {
        self.config.as_ref()
    }

    /// Aplica `config` si la fase y los datos lo permiten. Atómico: si
    /// [`validate_config`] rechaza, la configuración anterior se conserva
    /// (doc del mensaje `config`: "o entra entera o se rechaza entera").
    pub fn apply_config(&mut self, config: Config) -> Result<(), u8> {
        if self.phase != phase::SETUP {
            return Err(error::NOT_IN_SETUP_PHASE);
        }
        validate_config(&config, &self.caps)?;
        self.config = Some(config);
        Ok(())
    }

    /// Procesa un mandato de `control`. Ver el doc del módulo para lo que
    /// deliberadamente no comprueba (`drx_link_down`, política de `fault`).
    pub fn handle_command(&mut self, command: u8) -> Result<(), u8> {
        match command {
            command::ENTER_SETUP => {
                self.phase = phase::SETUP;
                Ok(())
            }
            command::START => {
                if self.config.is_none() {
                    return Err(error::NOT_CONFIGURED);
                }
                self.phase = phase::RUNNING;
                Ok(())
            }
            command::STOP => {
                self.phase = phase::SETUP;
                Ok(())
            }
            command::REQUEST_STATUS
            | command::REQUEST_CONFIG
            | command::REQUEST_CAPABILITIES
            | command::RESET_COUNTERS => Ok(()),
            _ => Err(error::UNKNOWN_MESSAGE),
        }
    }

    /// Transición mecánica a `fault`. Quién decide cuándo llamarla (BITE,
    /// fallo de enlace DRx, lo que sea) es responsabilidad de quien la
    /// invoque, no de este módulo.
    pub fn enter_fault(&mut self) {
        self.phase = phase::FAULT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamula_contract::dsp_rcp::{
        clutter_filter, dealias_mode, estimator, moment_kind, sweep_mode,
    };

    fn full_capabilities() -> Capabilities {
        Capabilities {
            moment_mask: 0xFFFF,
            dealias_mask: 1 << dealias_mode::NONE,
            estimator_mask: 1 << estimator::PULSE_PAIR,
            max_gates: 2000,
            max_pulses: 128,
            n_rx_channels: 2,
            pad0: 0,
        }
    }

    fn valid_config() -> Config {
        Config {
            seq: 1,
            moment_mask: 1 << moment_kind::UZ,
            n_pulses: 64,
            n_gates: 500,
            clutter_filter: clutter_filter::NONE,
            dealias_mode: dealias_mode::NONE,
            sweep_mode: sweep_mode::PPI,
            estimator: estimator::PULSE_PAIR,
            rfi_filter: 0,
            range_dealias: 0,
            prf_ratio_num: 0,
            prf_ratio_den: 0,
            start_range_m: 0.0,
            gate_spacing_m: 250.0,
            prf_hz: 300.0,
            sqi_threshold: 0.4,
            sig_threshold: 3.0,
            ccor_threshold: 20.0,
            log_threshold: -10.0,
            clutter_width_ms: 1.0,
            radar_constant_db: 65.0,
            noise_floor_dbm: -108.0,
            receiver_gain_db: 40.0,
            zdr_offset_db: 0.0,
            phidp_offset_deg: 0.0,
            wavelength_m: 0.1,
            pad0: 0,
        }
    }

    #[test]
    fn starts_in_setup_without_config() {
        let session = Session::new(full_capabilities());
        assert_eq!(session.phase(), phase::SETUP);
        assert!(session.config().is_none());
    }

    #[test]
    fn accepts_config_in_setup() {
        let mut session = Session::new(full_capabilities());
        assert_eq!(session.apply_config(valid_config()), Ok(()));
        assert!(session.config().is_some());
    }

    #[test]
    fn rejects_config_outside_setup() {
        let mut session = Session::new(full_capabilities());
        session.apply_config(valid_config()).unwrap();
        session.handle_command(command::START).unwrap();
        assert_eq!(session.phase(), phase::RUNNING);

        let mut second = valid_config();
        second.seq = 2;
        assert_eq!(session.apply_config(second), Err(error::NOT_IN_SETUP_PHASE));
    }

    #[test]
    fn invalid_config_is_rejected_and_previous_one_kept() {
        let mut session = Session::new(full_capabilities());
        session.apply_config(valid_config()).unwrap();

        let mut bad = valid_config();
        bad.seq = 99;
        bad.sqi_threshold = 5.0; // fuera de 0..=1
        assert_eq!(
            session.apply_config(bad),
            Err(error::THRESHOLD_OUT_OF_RANGE)
        );
        let seq = session.config().unwrap().seq;
        assert_eq!(seq, 1); // no la sobrescribió
    }

    #[test]
    fn start_without_config_is_rejected() {
        let mut session = Session::new(full_capabilities());
        assert_eq!(
            session.handle_command(command::START),
            Err(error::NOT_CONFIGURED)
        );
        assert_eq!(session.phase(), phase::SETUP);
    }

    #[test]
    fn start_after_config_enters_running() {
        let mut session = Session::new(full_capabilities());
        session.apply_config(valid_config()).unwrap();
        assert_eq!(session.handle_command(command::START), Ok(()));
        assert_eq!(session.phase(), phase::RUNNING);
    }

    #[test]
    fn stop_returns_to_setup_and_keeps_config() {
        let mut session = Session::new(full_capabilities());
        session.apply_config(valid_config()).unwrap();
        session.handle_command(command::START).unwrap();
        assert_eq!(session.handle_command(command::STOP), Ok(()));
        assert_eq!(session.phase(), phase::SETUP);
        assert!(session.config().is_some());
    }

    #[test]
    fn enter_setup_from_fault_returns_to_setup() {
        let mut session = Session::new(full_capabilities());
        session.apply_config(valid_config()).unwrap();
        session.enter_fault();
        assert_eq!(session.phase(), phase::FAULT);
        assert_eq!(session.handle_command(command::ENTER_SETUP), Ok(()));
        assert_eq!(session.phase(), phase::SETUP);
        assert!(session.config().is_some());
    }

    #[test]
    fn readonly_commands_do_not_change_phase() {
        let mut session = Session::new(full_capabilities());
        for cmd in [
            command::REQUEST_STATUS,
            command::REQUEST_CONFIG,
            command::REQUEST_CAPABILITIES,
            command::RESET_COUNTERS,
        ] {
            assert_eq!(session.handle_command(cmd), Ok(()));
            assert_eq!(session.phase(), phase::SETUP);
        }
    }

    #[test]
    fn out_of_table_command_is_rejected() {
        let mut session = Session::new(full_capabilities());
        assert_eq!(session.handle_command(200), Err(error::UNKNOWN_MESSAGE));
    }
}
