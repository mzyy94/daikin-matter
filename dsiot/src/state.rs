//! Device state machine for HVAC control.
//!
//! This module provides type-safe state transitions for power and mode control,
//! ensuring invalid state combinations are prevented at compile time or runtime.

use crate::protocol::status::DaikinStatus;
use crate::types::Mode;

/// Represents the device's power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    Off,
    On,
}

impl PowerState {
    /// Extract power state from DaikinStatus.
    pub fn from_status(status: &DaikinStatus) -> Option<Self> {
        status.power.get_f32().map(|v| {
            if v >= 1.0 {
                PowerState::On
            } else {
                PowerState::Off
            }
        })
    }

    /// Convert to f32 value for protocol.
    pub fn to_f32(self) -> f32 {
        match self {
            PowerState::Off => 0.0,
            PowerState::On => 1.0,
        }
    }
}

/// Error type for invalid state transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateTransitionError {
    /// Invalid mode value.
    InvalidMode,
    /// Cannot determine current state.
    UnknownCurrentState,
}

impl core::fmt::Display for StateTransitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidMode => write!(f, "Invalid mode value"),
            Self::UnknownCurrentState => write!(f, "Cannot determine current device state"),
        }
    }
}

impl core::error::Error for StateTransitionError {}

/// High-level device state combining power and mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceState {
    pub power: PowerState,
    pub mode: Mode,
}

impl DeviceState {
    /// Create a new device state.
    pub fn new(power: PowerState, mode: Mode) -> Self {
        Self { power, mode }
    }

    /// Extract current device state from DaikinStatus.
    pub fn from_status(status: &DaikinStatus) -> Option<Self> {
        let power = PowerState::from_status(status)?;
        let mode = status.mode.get_enum()?;
        Some(Self { power, mode })
    }

    /// Validate and compute new state from transition request.
    ///
    /// Rules:
    /// - Power can always be turned off or on
    /// - Mode can be changed regardless of power state
    pub fn transition(
        &self,
        new_power: Option<PowerState>,
        new_mode: Option<Mode>,
    ) -> Result<DeviceState, StateTransitionError> {
        let target_power = new_power.unwrap_or(self.power);
        let target_mode = new_mode.unwrap_or(self.mode);

        Ok(DeviceState {
            power: target_power,
            mode: target_mode,
        })
    }

    /// Apply this state to a DaikinStatus.
    pub fn apply_to_status(&self, status: &mut DaikinStatus) {
        status.power.set_value(self.power.to_f32());
        status.mode.set_value(self.mode);
    }
}

/// Builder for constructing state transitions.
#[derive(Debug, Clone)]
pub struct StateTransition {
    power: Option<PowerState>,
    mode: Option<Mode>,
}

impl StateTransition {
    /// Create a new empty transition.
    pub fn new() -> Self {
        Self {
            power: None,
            mode: None,
        }
    }

    /// Set power state change.
    pub fn power(mut self, power: PowerState) -> Self {
        self.power = Some(power);
        self
    }

    /// Set mode change.
    pub fn mode(mut self, mode: Mode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Turn device on.
    pub fn turn_on(self) -> Self {
        self.power(PowerState::On)
    }

    /// Turn device off.
    pub fn turn_off(self) -> Self {
        self.power(PowerState::Off)
    }

    /// Apply transition to current state.
    pub fn apply(&self, current: &DeviceState) -> Result<DeviceState, StateTransitionError> {
        current.transition(self.power, self.mode)
    }

    /// Apply transition directly to DaikinStatus.
    pub fn apply_to_status(
        &self,
        status: &mut DaikinStatus,
    ) -> Result<DeviceState, StateTransitionError> {
        let current =
            DeviceState::from_status(status).ok_or(StateTransitionError::UnknownCurrentState)?;
        let new_state = self.apply(&current)?;
        new_state.apply_to_status(status);
        Ok(new_state)
    }
}

impl Default for StateTransition {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_state(power: PowerState, mode: Mode) -> DeviceState {
        DeviceState::new(power, mode)
    }

    #[test]
    fn test_power_off_to_on() {
        let state = make_test_state(PowerState::Off, Mode::Cooling);
        let result = state.transition(Some(PowerState::On), None);
        assert!(result.is_ok());
        let new_state = result.unwrap();
        assert_eq!(new_state.power, PowerState::On);
        assert_eq!(new_state.mode, Mode::Cooling);
    }

    #[test]
    fn test_power_on_to_off() {
        let state = make_test_state(PowerState::On, Mode::Heating);
        let result = state.transition(Some(PowerState::Off), None);
        assert!(result.is_ok());
        let new_state = result.unwrap();
        assert_eq!(new_state.power, PowerState::Off);
        assert_eq!(new_state.mode, Mode::Heating);
    }

    #[test]
    fn test_mode_change_when_on() {
        let state = make_test_state(PowerState::On, Mode::Cooling);
        let result = state.transition(None, Some(Mode::Heating));
        assert!(result.is_ok());
        let new_state = result.unwrap();
        assert_eq!(new_state.power, PowerState::On);
        assert_eq!(new_state.mode, Mode::Heating);
    }

    #[test]
    fn test_mode_change_when_off_allowed() {
        let state = make_test_state(PowerState::Off, Mode::Cooling);
        let result = state.transition(None, Some(Mode::Heating));
        assert!(result.is_ok());
        let new_state = result.unwrap();
        assert_eq!(new_state.power, PowerState::Off);
        assert_eq!(new_state.mode, Mode::Heating);
    }

    #[test]
    fn test_power_on_and_mode_change() {
        let state = make_test_state(PowerState::Off, Mode::Cooling);
        let result = state.transition(Some(PowerState::On), Some(Mode::Auto));
        assert!(result.is_ok());
        let new_state = result.unwrap();
        assert_eq!(new_state.power, PowerState::On);
        assert_eq!(new_state.mode, Mode::Auto);
    }

    #[test]
    fn test_power_off_and_mode_change_allowed() {
        let state = make_test_state(PowerState::On, Mode::Cooling);
        let result = state.transition(Some(PowerState::Off), Some(Mode::Heating));
        assert!(result.is_ok());
        let new_state = result.unwrap();
        assert_eq!(new_state.power, PowerState::Off);
        assert_eq!(new_state.mode, Mode::Heating);
    }

    #[test]
    fn test_state_transition_builder() {
        let state = make_test_state(PowerState::Off, Mode::Fan);

        // Turn on
        let result = StateTransition::new().turn_on().apply(&state);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().power, PowerState::On);

        // Turn on and change mode
        let result = StateTransition::new()
            .turn_on()
            .mode(Mode::Cooling)
            .apply(&state);
        assert!(result.is_ok());
        let new_state = result.unwrap();
        assert_eq!(new_state.power, PowerState::On);
        assert_eq!(new_state.mode, Mode::Cooling);
    }

    #[test]
    fn test_no_change() {
        let state = make_test_state(PowerState::On, Mode::Cooling);
        let result = state.transition(None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), state);
    }
}
