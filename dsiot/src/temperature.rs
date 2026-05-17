//! Mode-dependent temperature management.
//!
//! This module provides type-safe temperature handling that ensures
//! temperatures are applied correctly based on the current operating mode.

use crate::protocol::status::DaikinStatus;
use crate::types::Mode;

/// Represents valid temperature targets for a given mode.
#[derive(Debug, Clone, PartialEq)]
pub enum TemperatureTarget {
    /// Heating mode target temperature.
    Heating(f32),
    /// Cooling mode target temperature.
    Cooling(f32),
    /// Auto mode relative temperature offset (-5 to +5).
    Auto(f32),
    /// Fan or Dehumidify mode - temperature not applicable.
    None,
}

/// Error type for temperature operations.
#[derive(Debug, Clone, PartialEq)]
pub enum TemperatureError {
    /// Temperature type doesn't match current mode.
    ModeMismatch {
        expected_mode: &'static str,
        actual_mode: Option<Mode>,
    },
    /// Cannot determine current mode.
    UnknownMode,
}

impl core::fmt::Display for TemperatureError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ModeMismatch {
                expected_mode,
                actual_mode,
            } => {
                write!(
                    f,
                    "Temperature type {} doesn't match mode {:?}",
                    expected_mode, actual_mode
                )
            }
            Self::UnknownMode => write!(f, "Cannot determine current operating mode"),
        }
    }
}

impl core::error::Error for TemperatureError {}

impl TemperatureTarget {
    /// Create a heating temperature target.
    pub fn heating(temp: f32) -> Self {
        Self::Heating(temp)
    }

    /// Create a cooling temperature target.
    pub fn cooling(temp: f32) -> Self {
        Self::Cooling(temp)
    }

    /// Create an auto mode temperature offset (-5 to +5).
    pub fn auto(offset: f32) -> Self {
        Self::Auto(offset)
    }

    /// Extract current temperature target from status based on mode.
    pub fn from_status(status: &DaikinStatus) -> Option<Self> {
        let mode = status.mode.get_enum()?;

        match mode {
            Mode::Heating => status.temperature.heating.get_f32().map(Self::Heating),
            Mode::Cooling => status.temperature.cooling.get_f32().map(Self::Cooling),
            Mode::Auto => status.temperature.automatic.get_f32().map(Self::Auto),
            Mode::Fan | Mode::Dehumidify => Some(Self::None),
            Mode::Unknown => None,
        }
    }

    /// Check if this temperature target is appropriate for the given mode.
    pub fn is_valid_for_mode(&self, mode: &Mode) -> bool {
        matches!(
            (self, mode),
            (Self::Heating(_), Mode::Heating)
                | (Self::Cooling(_), Mode::Cooling)
                | (Self::Auto(_), Mode::Auto)
                | (Self::None, Mode::Fan | Mode::Dehumidify)
        )
    }

    /// Apply this temperature to status.
    ///
    /// This method applies the temperature regardless of mode validation.
    /// Use `apply_validated` if you want mode checking.
    pub fn apply_to_status(&self, status: &mut DaikinStatus) {
        match self {
            Self::Heating(temp) => {
                status.temperature.heating.set_value(*temp);
            }
            Self::Cooling(temp) => {
                status.temperature.cooling.set_value(*temp);
            }
            Self::Auto(offset) => {
                status.temperature.automatic.set_value(*offset);
            }
            Self::None => {
                // No temperature to set
            }
        }
    }

    /// Apply this temperature to status with mode validation.
    ///
    /// Returns an error if the temperature type doesn't match the current mode.
    pub fn apply_validated(&self, status: &mut DaikinStatus) -> Result<(), TemperatureError> {
        let mode = status
            .mode
            .get_enum()
            .ok_or(TemperatureError::UnknownMode)?;

        if !self.is_valid_for_mode(&mode) {
            return Err(TemperatureError::ModeMismatch {
                expected_mode: self.mode_name(),
                actual_mode: Some(mode),
            });
        }

        self.apply_to_status(status);
        Ok(())
    }

    /// Get the mode name for this temperature target.
    fn mode_name(&self) -> &'static str {
        match self {
            Self::Heating(_) => "Heating",
            Self::Cooling(_) => "Cooling",
            Self::Auto(_) => "Auto",
            Self::None => "None",
        }
    }

    /// Get the temperature value if applicable.
    pub fn value(&self) -> Option<f32> {
        match self {
            Self::Heating(t) | Self::Cooling(t) | Self::Auto(t) => Some(*t),
            Self::None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_target_creation() {
        let heating = TemperatureTarget::heating(22.0);
        assert_eq!(heating.value(), Some(22.0));

        let cooling = TemperatureTarget::cooling(26.0);
        assert_eq!(cooling.value(), Some(26.0));

        let auto = TemperatureTarget::auto(2.0);
        assert_eq!(auto.value(), Some(2.0));

        let none = TemperatureTarget::None;
        assert_eq!(none.value(), None);
    }

    #[test]
    fn test_mode_validation() {
        let heating = TemperatureTarget::heating(22.0);
        assert!(heating.is_valid_for_mode(&Mode::Heating));
        assert!(!heating.is_valid_for_mode(&Mode::Cooling));
        assert!(!heating.is_valid_for_mode(&Mode::Auto));
        assert!(!heating.is_valid_for_mode(&Mode::Fan));

        let cooling = TemperatureTarget::cooling(26.0);
        assert!(cooling.is_valid_for_mode(&Mode::Cooling));
        assert!(!cooling.is_valid_for_mode(&Mode::Heating));
        assert!(!cooling.is_valid_for_mode(&Mode::Auto));
        assert!(!cooling.is_valid_for_mode(&Mode::Fan));

        let auto = TemperatureTarget::auto(0.0);
        assert!(auto.is_valid_for_mode(&Mode::Auto));
        assert!(!auto.is_valid_for_mode(&Mode::Heating));
        assert!(!auto.is_valid_for_mode(&Mode::Cooling));

        let none = TemperatureTarget::None;
        assert!(none.is_valid_for_mode(&Mode::Fan));
        assert!(none.is_valid_for_mode(&Mode::Dehumidify));
        assert!(!none.is_valid_for_mode(&Mode::Heating));
    }

    #[test]
    fn test_mode_mismatch_error_display() {
        let err = TemperatureError::ModeMismatch {
            expected_mode: "Heating",
            actual_mode: Some(Mode::Cooling),
        };
        assert!(err.to_string().contains("Heating"));
        assert!(err.to_string().contains("Cooling"));
    }
}
