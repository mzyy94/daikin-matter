use dsiot::{
    DaikinStatus, Mode, PowerState, SetValueError, StateTransition, TemperatureTarget,
    ValueConstraints,
};
use rs_matter::dm::clusters::decl::thermostat;
use rs_matter::dm::{Cluster, Dataver, InvokeContext, ReadContext, WriteContext};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::{Nullable, TLVBuilderParent};
use rs_matter::with;

use crate::device::Device;

/// Map a value-write failure to a Matter error.
fn set_err(e: SetValueError) -> Error {
    warn!("Failed to apply temperature: {e}");
    Error::from(ErrorCode::InvalidState)
}

pub struct ThermostatHandler {
    pub(crate) dataver: Dataver,
    device: Device,
}

impl ThermostatHandler {
    pub const CLUSTER: Cluster<'static> = thermostat::FULL_CLUSTER
        .with_revision(7)
        .with_features(
            thermostat::Feature::HEATING.bits()
                | thermostat::Feature::COOLING.bits()
                | thermostat::Feature::AUTO_MODE.bits(),
        )
        .with_attrs(with!(
            required;
            thermostat::AttributeId::LocalTemperature
            | thermostat::AttributeId::OutdoorTemperature
            | thermostat::AttributeId::SystemMode
            | thermostat::AttributeId::OccupiedCoolingSetpoint
            | thermostat::AttributeId::OccupiedHeatingSetpoint
            | thermostat::AttributeId::AbsMinHeatSetpointLimit
            | thermostat::AttributeId::AbsMaxHeatSetpointLimit
            | thermostat::AttributeId::AbsMinCoolSetpointLimit
            | thermostat::AttributeId::AbsMaxCoolSetpointLimit
            | thermostat::AttributeId::MinHeatSetpointLimit
            | thermostat::AttributeId::MaxHeatSetpointLimit
            | thermostat::AttributeId::MinCoolSetpointLimit
            | thermostat::AttributeId::MaxCoolSetpointLimit
            | thermostat::AttributeId::ControlSequenceOfOperation
            | thermostat::AttributeId::ThermostatRunningMode
            | thermostat::AttributeId::ThermostatRunningState
        ))
        .with_cmds(with!(thermostat::CommandId::SetpointRaiseLower));

    pub fn new(dataver: Dataver, device: Device) -> Self {
        Self { dataver, device }
    }

    fn get_status(&self) -> Result<DaikinStatus, Error> {
        self.device.get_status().map_err(|e| {
            warn!("Failed to get status: {e}");
            Error::from(ErrorCode::Busy)
        })
    }

    fn update(&self, status: DaikinStatus) -> Result<(), Error> {
        self.device.update(status).map_err(|e| {
            warn!("Failed to update: {e}");
            Error::from(ErrorCode::Busy)
        })
    }
}

fn system_mode_from_status(status: &DaikinStatus) -> thermostat::SystemModeEnum {
    match PowerState::from_status(status) {
        Some(PowerState::Off) | None => thermostat::SystemModeEnum::Off,
        Some(PowerState::On) => match status.mode.get_enum() {
            Some(Mode::Auto) => thermostat::SystemModeEnum::Auto,
            Some(Mode::Cooling) => thermostat::SystemModeEnum::Cool,
            Some(Mode::Heating) => thermostat::SystemModeEnum::Heat,
            Some(Mode::Fan) => thermostat::SystemModeEnum::FanOnly,
            Some(Mode::Dehumidify) => thermostat::SystemModeEnum::Dry,
            _ => thermostat::SystemModeEnum::Off,
        },
    }
}

/// Convert dsiot f32 °C to Matter 0.01°C i16.
fn temp_to_matter(celsius: f32) -> i16 {
    (celsius * 100.0) as i16
}

/// Convert Matter 0.01°C i16 to dsiot f32 °C.
fn temp_from_matter(value: i16) -> f32 {
    value as f32 / 100.0
}

/// Validate and round temperature to the nearest step within the allowed range.
fn validate_temp(temp: f32, constraints: &ValueConstraints) -> Result<f32, Error> {
    if temp < constraints.min || temp > constraints.max {
        return Err(ErrorCode::ConstraintError.into());
    }
    let stepped =
        ((temp - constraints.min) / constraints.step).round() * constraints.step + constraints.min;
    Ok(stepped.clamp(constraints.min, constraints.max))
}

fn setpoint_limit(
    constraints: Option<ValueConstraints>,
    fallback_celsius: f32,
    value: impl FnOnce(&ValueConstraints) -> f32,
) -> i16 {
    match constraints {
        Some(constraints) => temp_to_matter(value(&constraints)),
        None => temp_to_matter(fallback_celsius),
    }
}

impl thermostat::ClusterHandler for ThermostatHandler {
    const CLUSTER: Cluster<'static> = Self::CLUSTER;

    fn dataver(&self) -> u32 {
        self.dataver.get()
    }
    fn dataver_changed(&self) {
        self.dataver.changed();
    }

    fn local_temperature(&self, _ctx: impl ReadContext) -> Result<Nullable<i16>, Error> {
        let status = self.get_status()?;
        match status.sensors.temperature.get_f32() {
            Some(t) => Ok(Nullable::some(temp_to_matter(t))),
            None => Ok(Nullable::none()),
        }
    }

    fn outdoor_temperature(&self, _ctx: impl ReadContext) -> Result<Nullable<i16>, Error> {
        let status = self.get_status()?;
        match status.sensors.outdoor_temperature.get_f32() {
            Some(t) => Ok(Nullable::some(temp_to_matter(t))),
            None => Ok(Nullable::none()),
        }
    }

    fn system_mode(&self, _ctx: impl ReadContext) -> Result<thermostat::SystemModeEnum, Error> {
        let status = self.get_status()?;
        Ok(system_mode_from_status(&status))
    }

    fn set_system_mode(
        &self,
        _ctx: impl WriteContext,
        value: thermostat::SystemModeEnum,
    ) -> Result<(), Error> {
        let mut status = self.get_status()?;
        let transition = match value {
            thermostat::SystemModeEnum::Off => StateTransition::new().turn_off(),
            thermostat::SystemModeEnum::Auto => StateTransition::new().turn_on().mode(Mode::Auto),
            thermostat::SystemModeEnum::Cool => {
                StateTransition::new().turn_on().mode(Mode::Cooling)
            }
            thermostat::SystemModeEnum::Heat => {
                StateTransition::new().turn_on().mode(Mode::Heating)
            }
            thermostat::SystemModeEnum::FanOnly => StateTransition::new().turn_on().mode(Mode::Fan),
            thermostat::SystemModeEnum::Dry => {
                StateTransition::new().turn_on().mode(Mode::Dehumidify)
            }
            _ => return Err(ErrorCode::ConstraintError.into()),
        };
        transition.apply_to_status(&mut status).map_err(|e| {
            warn!("State transition failed: {e}");
            Error::from(ErrorCode::InvalidState)
        })?;
        debug!("Thermostat: system_mode → {:?}", value);
        self.update(status)?;
        self.dataver.changed();
        Ok(())
    }

    fn occupied_cooling_setpoint(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        match status.temperature.cooling.get_f32() {
            Some(t) => Ok(temp_to_matter(t)),
            None => Ok(2600), // 26.0°C
        }
    }

    fn set_occupied_cooling_setpoint(
        &self,
        _ctx: impl WriteContext,
        value: i16,
    ) -> Result<(), Error> {
        let mut status = self.get_status()?;
        let temp = temp_from_matter(value);
        let temp = match ValueConstraints::from_item(&status.temperature.cooling) {
            Some(c) => validate_temp(temp, &c)?,
            None => temp,
        };
        TemperatureTarget::cooling(temp)
            .apply_to_status(&mut status)
            .map_err(set_err)?;
        debug!("Thermostat: cooling setpoint → {temp}°C");
        self.update(status)?;
        self.dataver.changed();
        Ok(())
    }

    fn occupied_heating_setpoint(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        match status.temperature.heating.get_f32() {
            Some(t) => Ok(temp_to_matter(t)),
            None => Ok(2000), // 20.0°C
        }
    }

    fn set_occupied_heating_setpoint(
        &self,
        _ctx: impl WriteContext,
        value: i16,
    ) -> Result<(), Error> {
        let mut status = self.get_status()?;
        let temp = temp_from_matter(value);
        let temp = match ValueConstraints::from_item(&status.temperature.heating) {
            Some(c) => validate_temp(temp, &c)?,
            None => temp,
        };
        TemperatureTarget::heating(temp)
            .apply_to_status(&mut status)
            .map_err(set_err)?;
        debug!("Thermostat: heating setpoint → {temp}°C");
        self.update(status)?;
        self.dataver.changed();
        Ok(())
    }

    fn abs_min_heat_setpoint_limit(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        Ok(setpoint_limit(
            ValueConstraints::from_item(&status.temperature.heating),
            7.0,
            |c| c.min,
        ))
    }

    fn abs_max_heat_setpoint_limit(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        Ok(setpoint_limit(
            ValueConstraints::from_item(&status.temperature.heating),
            30.0,
            |c| c.max,
        ))
    }

    fn abs_min_cool_setpoint_limit(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        Ok(setpoint_limit(
            ValueConstraints::from_item(&status.temperature.cooling),
            16.0,
            |c| c.min,
        ))
    }

    fn abs_max_cool_setpoint_limit(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        Ok(setpoint_limit(
            ValueConstraints::from_item(&status.temperature.cooling),
            32.0,
            |c| c.max,
        ))
    }

    fn min_heat_setpoint_limit(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        Ok(setpoint_limit(
            ValueConstraints::from_item(&status.temperature.heating),
            7.0,
            |c| c.min,
        ))
    }

    fn max_heat_setpoint_limit(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        Ok(setpoint_limit(
            ValueConstraints::from_item(&status.temperature.heating),
            30.0,
            |c| c.max,
        ))
    }

    fn min_cool_setpoint_limit(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        Ok(setpoint_limit(
            ValueConstraints::from_item(&status.temperature.cooling),
            16.0,
            |c| c.min,
        ))
    }

    fn max_cool_setpoint_limit(&self, _ctx: impl ReadContext) -> Result<i16, Error> {
        let status = self.get_status()?;
        Ok(setpoint_limit(
            ValueConstraints::from_item(&status.temperature.cooling),
            32.0,
            |c| c.max,
        ))
    }

    fn control_sequence_of_operation(
        &self,
        _ctx: impl ReadContext,
    ) -> Result<thermostat::ControlSequenceOfOperationEnum, Error> {
        Ok(thermostat::ControlSequenceOfOperationEnum::CoolingAndHeating)
    }

    fn set_control_sequence_of_operation(
        &self,
        _ctx: impl WriteContext,
        _value: thermostat::ControlSequenceOfOperationEnum,
    ) -> Result<(), Error> {
        Err(ErrorCode::InvalidAction.into())
    }

    fn thermostat_running_mode(
        &self,
        _ctx: impl ReadContext,
    ) -> Result<thermostat::ThermostatRunningModeEnum, Error> {
        let status = self.get_status()?;
        match PowerState::from_status(&status) {
            Some(PowerState::On) => match status.mode.get_enum() {
                Some(Mode::Cooling) | Some(Mode::Dehumidify) => {
                    Ok(thermostat::ThermostatRunningModeEnum::Cool)
                }
                Some(Mode::Heating) => Ok(thermostat::ThermostatRunningModeEnum::Heat),
                _ => Ok(thermostat::ThermostatRunningModeEnum::Off),
            },
            _ => Ok(thermostat::ThermostatRunningModeEnum::Off),
        }
    }

    fn thermostat_running_state(
        &self,
        _ctx: impl ReadContext,
    ) -> Result<thermostat::RelayStateBitmap, Error> {
        let status = self.get_status()?;
        match PowerState::from_status(&status) {
            Some(PowerState::On) => match status.mode.get_enum() {
                Some(Mode::Cooling) | Some(Mode::Dehumidify) => {
                    Ok(thermostat::RelayStateBitmap::COOL | thermostat::RelayStateBitmap::FAN)
                }
                Some(Mode::Heating) => {
                    Ok(thermostat::RelayStateBitmap::HEAT | thermostat::RelayStateBitmap::FAN)
                }
                Some(Mode::Fan) => Ok(thermostat::RelayStateBitmap::FAN),
                Some(Mode::Auto) => Ok(thermostat::RelayStateBitmap::FAN),
                _ => Ok(thermostat::RelayStateBitmap::empty()),
            },
            _ => Ok(thermostat::RelayStateBitmap::empty()),
        }
    }

    fn handle_setpoint_raise_lower(
        &self,
        _ctx: impl InvokeContext,
        req: thermostat::SetpointRaiseLowerRequest<'_>,
    ) -> Result<(), Error> {
        let mode = req.mode()?;
        let amount = req.amount()? as f32 * 0.1;
        let mut status = self.get_status()?;

        use thermostat::SetpointRaiseLowerModeEnum;
        if matches!(
            mode,
            SetpointRaiseLowerModeEnum::Cool | SetpointRaiseLowerModeEnum::Both
        ) {
            let current = status.temperature.cooling.get_f32().unwrap_or(26.0);
            let new_temp = current + amount;
            let new_temp = match ValueConstraints::from_item(&status.temperature.cooling) {
                Some(c) => validate_temp(new_temp, &c)?,
                None => new_temp,
            };
            TemperatureTarget::cooling(new_temp)
                .apply_to_status(&mut status)
                .map_err(set_err)?;
        }
        if matches!(
            mode,
            SetpointRaiseLowerModeEnum::Heat | SetpointRaiseLowerModeEnum::Both
        ) {
            let current = status.temperature.heating.get_f32().unwrap_or(20.0);
            let new_temp = current + amount;
            let new_temp = match ValueConstraints::from_item(&status.temperature.heating) {
                Some(c) => validate_temp(new_temp, &c)?,
                None => new_temp,
            };
            TemperatureTarget::heating(new_temp)
                .apply_to_status(&mut status)
                .map_err(set_err)?;
        }

        debug!(
            "Thermostat: setpoint_raise_lower mode={:?} amount={amount}°C",
            mode
        );
        self.update(status)?;
        self.dataver.changed();
        Ok(())
    }

    fn handle_set_weekly_schedule(
        &self,
        _ctx: impl InvokeContext,
        _req: thermostat::SetWeeklyScheduleRequest<'_>,
    ) -> Result<(), Error> {
        Err(ErrorCode::InvalidCommand.into())
    }

    fn handle_get_weekly_schedule<P: TLVBuilderParent>(
        &self,
        _ctx: impl InvokeContext,
        _req: thermostat::GetWeeklyScheduleRequest<'_>,
        _response: thermostat::GetWeeklyScheduleResponseBuilder<P>,
    ) -> Result<P, Error> {
        Err(ErrorCode::InvalidCommand.into())
    }

    fn handle_clear_weekly_schedule(&self, _ctx: impl InvokeContext) -> Result<(), Error> {
        Err(ErrorCode::InvalidCommand.into())
    }

    fn handle_set_active_schedule_request(
        &self,
        _ctx: impl InvokeContext,
        _req: thermostat::SetActiveScheduleRequestRequest<'_>,
    ) -> Result<(), Error> {
        Err(ErrorCode::InvalidCommand.into())
    }

    fn handle_set_active_preset_request(
        &self,
        _ctx: impl InvokeContext,
        _req: thermostat::SetActivePresetRequestRequest<'_>,
    ) -> Result<(), Error> {
        Err(ErrorCode::InvalidCommand.into())
    }

    fn handle_add_thermostat_suggestion<P: TLVBuilderParent>(
        &self,
        _ctx: impl InvokeContext,
        _req: thermostat::AddThermostatSuggestionRequest<'_>,
        _response: thermostat::AddThermostatSuggestionResponseBuilder<P>,
    ) -> Result<P, Error> {
        Err(ErrorCode::InvalidCommand.into())
    }

    fn handle_remove_thermostat_suggestion(
        &self,
        _ctx: impl InvokeContext,
        _req: thermostat::RemoveThermostatSuggestionRequest<'_>,
    ) -> Result<(), Error> {
        Err(ErrorCode::InvalidCommand.into())
    }

    fn handle_atomic_request<P: TLVBuilderParent>(
        &self,
        _ctx: impl InvokeContext,
        _req: thermostat::AtomicRequestRequest<'_>,
        _response: thermostat::AtomicResponseBuilder<P>,
    ) -> Result<P, Error> {
        Err(ErrorCode::InvalidCommand.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matter_temperatures_are_celsius_hundredths() {
        assert_eq!(temp_to_matter(18.0), 1800);
        assert_eq!(temp_to_matter(21.5), 2150);
        assert_eq!(temp_from_matter(2250), 22.5);
    }

    #[test]
    fn setpoint_limit_uses_constraints_when_present() {
        let constraints = ValueConstraints::new(18.0, 32.0, 0.5);

        assert_eq!(
            setpoint_limit(Some(constraints.clone()), 16.0, |c| c.min),
            1800
        );
        assert_eq!(setpoint_limit(Some(constraints), 32.0, |c| c.max), 3200);
    }

    #[test]
    fn setpoint_limit_uses_fallback_without_constraints() {
        assert_eq!(setpoint_limit(None, 16.0, |c| c.min), 1600);
    }

    #[test]
    fn validate_temp_rounds_to_nearest_supported_step() {
        let constraints = ValueConstraints::new(18.0, 32.0, 0.5);

        assert_eq!(validate_temp(22.2, &constraints).unwrap(), 22.0);
        assert_eq!(validate_temp(22.3, &constraints).unwrap(), 22.5);
    }
}
