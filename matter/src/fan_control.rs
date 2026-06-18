use dsiot::mapping::fan::{self, FanSpeed};
use dsiot::{
    AutoModeWindSpeed, DaikinStatus, HorizontalDirection, Mode, PowerState, SetValueError,
    StateTransition, VerticalDirection, WindSpeed,
};
use rs_matter::dm::clusters::decl::fan_control;
use rs_matter::dm::{Cluster, Dataver, InvokeContext, ReadContext, WriteContext};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::im::Percent;
use rs_matter::tlv::Nullable;
use rs_matter::with;

use crate::device::Device;

pub struct FanControlHandler {
    pub(crate) dataver: Dataver,
    device: Device,
}

const SPEED_MAX: u8 = 5;

impl FanControlHandler {
    pub const CLUSTER: Cluster<'static> = fan_control::FULL_CLUSTER
        .with_revision(4)
        .with_features(
            fan_control::Feature::MULTI_SPEED.bits()
                | fan_control::Feature::AUTO.bits()
                | fan_control::Feature::ROCKING.bits()
                | fan_control::Feature::WIND.bits(),
        )
        .with_attrs(with!(
            required;
            fan_control::AttributeId::FanMode
            | fan_control::AttributeId::FanModeSequence
            | fan_control::AttributeId::SpeedSetting
            | fan_control::AttributeId::SpeedMax
            | fan_control::AttributeId::SpeedCurrent
            | fan_control::AttributeId::RockSupport
            | fan_control::AttributeId::RockSetting
            | fan_control::AttributeId::WindSupport
            | fan_control::AttributeId::WindSetting
        ))
        .with_cmds(with!());

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

/// Get the current wind speed for the active HVAC mode.
fn current_wind_speed(status: &DaikinStatus) -> Option<WindSpeed> {
    let mode = status.mode.get_enum()?;
    match mode {
        Mode::Cooling => status.wind.cooling.speed.get_enum(),
        Mode::Heating => status.wind.heating.speed.get_enum(),
        Mode::Dehumidify => status.wind.dehumidify.speed.get_enum(),
        Mode::Auto => status.wind.auto.speed.get_enum().map(|s| match s {
            AutoModeWindSpeed::Auto => WindSpeed::Auto,
            AutoModeWindSpeed::Silent => WindSpeed::Silent,
            _ => WindSpeed::Auto,
        }),
        Mode::Fan => status.wind.fan.speed.get_enum(),
        _ => None,
    }
}

fn wind_speed_to_setting(speed: WindSpeed) -> u8 {
    match speed {
        WindSpeed::Silent | WindSpeed::Lev1 => 1,
        WindSpeed::Lev2 => 2,
        WindSpeed::Auto | WindSpeed::Lev3 => 3,
        WindSpeed::Lev4 => 4,
        WindSpeed::Lev5 => 5,
        _ => 0,
    }
}

fn setting_to_wind_speed(setting: u8) -> WindSpeed {
    match setting {
        1 => WindSpeed::Lev1,
        2 => WindSpeed::Lev2,
        3 => WindSpeed::Lev3,
        4 => WindSpeed::Lev4,
        5 => WindSpeed::Lev5,
        _ => WindSpeed::Auto,
    }
}

fn wind_speed_to_fan_mode(speed: WindSpeed, is_off: bool) -> fan_control::FanModeEnum {
    if is_off {
        return fan_control::FanModeEnum::Off;
    }
    match speed {
        WindSpeed::Auto => fan_control::FanModeEnum::Auto,
        WindSpeed::Silent | WindSpeed::Lev1 => fan_control::FanModeEnum::Low,
        WindSpeed::Lev2 | WindSpeed::Lev3 => fan_control::FanModeEnum::Medium,
        WindSpeed::Lev4 | WindSpeed::Lev5 => fan_control::FanModeEnum::High,
        _ => fan_control::FanModeEnum::Auto,
    }
}

/// Map a value-write failure to a Matter error.
fn set_err(e: SetValueError) -> Error {
    warn!("Failed to apply wind setting: {e}");
    Error::from(ErrorCode::InvalidState)
}

/// Apply wind speed to the current mode's wind settings.
fn apply_wind_speed(status: &mut DaikinStatus, speed: WindSpeed) -> Result<(), SetValueError> {
    let mode = status.mode.get_enum().unwrap_or(Mode::Auto);
    match mode {
        Mode::Cooling => status.wind.cooling.speed.set_value(speed),
        Mode::Heating => status.wind.heating.speed.set_value(speed),
        Mode::Dehumidify => status.wind.dehumidify.speed.set_value(speed),
        Mode::Auto => {
            let auto_speed = fan::fan_speed_to_auto_mode(&FanSpeed {
                percent: wind_speed_to_setting(speed) * 20,
                auto: speed == WindSpeed::Auto,
            });
            status.wind.auto.speed.set_value(auto_speed)
        }
        Mode::Fan => status.wind.fan.speed.set_value(speed),
        _ => Ok(()),
    }
}

/// Get vertical/horizontal direction for the active HVAC mode.
fn current_directions(
    status: &DaikinStatus,
) -> (Option<VerticalDirection>, Option<HorizontalDirection>) {
    let mode = status.mode.get_enum();
    match mode {
        Some(Mode::Cooling) => (
            status.wind.cooling.vertical_direction.get_enum(),
            status.wind.cooling.horizontal_direction.get_enum(),
        ),
        Some(Mode::Heating) => (
            status.wind.heating.vertical_direction.get_enum(),
            status.wind.heating.horizontal_direction.get_enum(),
        ),
        Some(Mode::Dehumidify) => (
            status.wind.dehumidify.vertical_direction.get_enum(),
            status.wind.dehumidify.horizontal_direction.get_enum(),
        ),
        Some(Mode::Auto) => (
            status.wind.auto.vertical_direction.get_enum(),
            status.wind.auto.horizontal_direction.get_enum(),
        ),
        Some(Mode::Fan) => (
            status.wind.fan.vertical_direction.get_enum(),
            status.wind.fan.horizontal_direction.get_enum(),
        ),
        _ => (None, None),
    }
}

/// Apply vertical/horizontal direction to the current mode's wind settings.
fn apply_directions(
    status: &mut DaikinStatus,
    vertical: VerticalDirection,
    horizontal: HorizontalDirection,
) -> Result<(), SetValueError> {
    let mode = status.mode.get_enum().unwrap_or(Mode::Auto);
    match mode {
        Mode::Cooling => {
            status.wind.cooling.vertical_direction.set_value(vertical)?;
            status
                .wind
                .cooling
                .horizontal_direction
                .set_value(horizontal)
        }
        Mode::Heating => {
            status.wind.heating.vertical_direction.set_value(vertical)?;
            status
                .wind
                .heating
                .horizontal_direction
                .set_value(horizontal)
        }
        Mode::Dehumidify => {
            status
                .wind
                .dehumidify
                .vertical_direction
                .set_value(vertical)?;
            status
                .wind
                .dehumidify
                .horizontal_direction
                .set_value(horizontal)
        }
        Mode::Auto => {
            status.wind.auto.vertical_direction.set_value(vertical)?;
            status.wind.auto.horizontal_direction.set_value(horizontal)
        }
        Mode::Fan => {
            status.wind.fan.vertical_direction.set_value(vertical)?;
            status.wind.fan.horizontal_direction.set_value(horizontal)
        }
        _ => Ok(()),
    }
}

impl fan_control::ClusterHandler for FanControlHandler {
    const CLUSTER: Cluster<'static> = Self::CLUSTER;

    fn dataver(&self) -> u32 {
        self.dataver.get()
    }
    fn dataver_changed(&self) {
        self.dataver.changed();
    }

    fn fan_mode(&self, _ctx: impl ReadContext) -> Result<fan_control::FanModeEnum, Error> {
        let status = self.get_status()?;
        let is_off = PowerState::from_status(&status) != Some(PowerState::On);
        let speed = current_wind_speed(&status).unwrap_or(WindSpeed::Auto);
        Ok(wind_speed_to_fan_mode(speed, is_off))
    }

    fn fan_mode_sequence(
        &self,
        _ctx: impl ReadContext,
    ) -> Result<fan_control::FanModeSequenceEnum, Error> {
        Ok(fan_control::FanModeSequenceEnum::OffLowMedHighAuto)
    }

    fn percent_setting(&self, _ctx: impl ReadContext) -> Result<Nullable<Percent>, Error> {
        let status = self.get_status()?;
        match current_wind_speed(&status) {
            Some(WindSpeed::Auto) => Ok(Nullable::none()),
            Some(s) => Ok(Nullable::some(wind_speed_to_setting(s) * 20)),
            None => Ok(Nullable::some(0)),
        }
    }

    fn percent_current(&self, _ctx: impl ReadContext) -> Result<Percent, Error> {
        let status = self.get_status()?;
        Ok(current_wind_speed(&status)
            .map(|s| wind_speed_to_setting(s) * 20)
            .unwrap_or(0))
    }

    fn speed_max(&self, _ctx: impl ReadContext) -> Result<u8, Error> {
        Ok(SPEED_MAX)
    }

    fn speed_setting(&self, _ctx: impl ReadContext) -> Result<Nullable<u8>, Error> {
        let status = self.get_status()?;
        match current_wind_speed(&status) {
            Some(WindSpeed::Auto) => Ok(Nullable::none()),
            Some(s) => Ok(Nullable::some(wind_speed_to_setting(s))),
            None => Ok(Nullable::some(0)),
        }
    }

    fn speed_current(&self, _ctx: impl ReadContext) -> Result<u8, Error> {
        let status = self.get_status()?;
        Ok(current_wind_speed(&status)
            .map(wind_speed_to_setting)
            .unwrap_or(0))
    }

    fn rock_support(&self, _ctx: impl ReadContext) -> Result<fan_control::RockBitmap, Error> {
        Ok(fan_control::RockBitmap::ROCK_UP_DOWN | fan_control::RockBitmap::ROCK_LEFT_RIGHT)
    }

    fn rock_setting(&self, _ctx: impl ReadContext) -> Result<fan_control::RockBitmap, Error> {
        let status = self.get_status()?;
        let (vert, horiz) = current_directions(&status);
        let mut bits = fan_control::RockBitmap::empty();
        if vert == Some(VerticalDirection::Swing) {
            bits |= fan_control::RockBitmap::ROCK_UP_DOWN;
        }
        if horiz == Some(HorizontalDirection::Swing) {
            bits |= fan_control::RockBitmap::ROCK_LEFT_RIGHT;
        }
        Ok(bits)
    }

    fn wind_support(&self, _ctx: impl ReadContext) -> Result<fan_control::WindBitmap, Error> {
        Ok(fan_control::WindBitmap::SLEEP_WIND | fan_control::WindBitmap::NATURAL_WIND)
    }

    fn wind_setting(&self, _ctx: impl ReadContext) -> Result<fan_control::WindBitmap, Error> {
        let status = self.get_status()?;
        let speed = current_wind_speed(&status);
        let (vert, _) = current_directions(&status);
        let mut bits = fan_control::WindBitmap::empty();
        if speed == Some(WindSpeed::Silent) {
            bits |= fan_control::WindBitmap::SLEEP_WIND;
        }
        let mode = status.mode.get_enum();
        if vert == Some(VerticalDirection::Nice) && mode != Some(Mode::Fan) {
            bits |= fan_control::WindBitmap::NATURAL_WIND;
        }
        Ok(bits)
    }

    fn set_fan_mode(
        &self,
        _ctx: impl WriteContext,
        value: fan_control::FanModeEnum,
    ) -> Result<(), Error> {
        let speed = match value {
            fan_control::FanModeEnum::Off => {
                let mut status = self.get_status()?;
                StateTransition::new()
                    .power(PowerState::Off)
                    .apply_to_status(&mut status)
                    .map_err(|e| {
                        warn!("State transition failed: {e}");
                        Error::from(ErrorCode::InvalidState)
                    })?;
                debug!("FanControl: fan_mode → Off (power off)");
                self.update(status)?;
                self.dataver.changed();
                return Ok(());
            }
            fan_control::FanModeEnum::Low => WindSpeed::Lev1,
            fan_control::FanModeEnum::Medium => WindSpeed::Lev3,
            fan_control::FanModeEnum::High => WindSpeed::Lev5,
            fan_control::FanModeEnum::On => WindSpeed::Lev3,
            fan_control::FanModeEnum::Auto | fan_control::FanModeEnum::Smart => WindSpeed::Auto,
        };
        let mut status = self.get_status()?;
        apply_wind_speed(&mut status, speed).map_err(set_err)?;
        debug!("FanControl: fan_mode → {:?}", value);
        self.update(status)?;
        self.dataver.changed();
        Ok(())
    }

    fn set_percent_setting(
        &self,
        _ctx: impl WriteContext,
        value: Nullable<Percent>,
    ) -> Result<(), Error> {
        let opt: Option<Percent> = value.into();
        let speed = match opt {
            None => WindSpeed::Auto,
            Some(pct) => fan::fan_speed_to_speed(&FanSpeed {
                percent: pct,
                auto: false,
            }),
        };
        let mut status = self.get_status()?;
        apply_wind_speed(&mut status, speed).map_err(set_err)?;
        debug!("FanControl: percent_setting → {:?}", opt);
        self.update(status)?;
        self.dataver.changed();
        Ok(())
    }

    fn set_speed_setting(&self, _ctx: impl WriteContext, value: Nullable<u8>) -> Result<(), Error> {
        let opt: Option<u8> = value.into();
        let speed = match opt {
            None => WindSpeed::Auto,
            Some(s) => setting_to_wind_speed(s),
        };
        let mut status = self.get_status()?;
        apply_wind_speed(&mut status, speed).map_err(set_err)?;
        debug!("FanControl: speed_setting → {:?}", opt);
        self.update(status)?;
        self.dataver.changed();
        Ok(())
    }

    fn set_rock_setting(
        &self,
        _ctx: impl WriteContext,
        value: fan_control::RockBitmap,
    ) -> Result<(), Error> {
        let vertical = if value.contains(fan_control::RockBitmap::ROCK_UP_DOWN) {
            VerticalDirection::Swing
        } else {
            VerticalDirection::Auto
        };
        let horizontal = if value.contains(fan_control::RockBitmap::ROCK_LEFT_RIGHT) {
            HorizontalDirection::Swing
        } else {
            HorizontalDirection::Auto
        };
        let mut status = self.get_status()?;
        apply_directions(&mut status, vertical, horizontal).map_err(set_err)?;
        debug!("FanControl: rock_setting → {:?}", value);
        self.update(status)?;
        self.dataver.changed();
        Ok(())
    }

    fn set_wind_setting(
        &self,
        _ctx: impl WriteContext,
        value: fan_control::WindBitmap,
    ) -> Result<(), Error> {
        let mut status = self.get_status()?;
        if value.contains(fan_control::WindBitmap::SLEEP_WIND) {
            apply_wind_speed(&mut status, WindSpeed::Silent).map_err(set_err)?;
        }
        let is_fan_mode = status.mode.get_enum() == Some(Mode::Fan);
        let sleep = value.contains(fan_control::WindBitmap::SLEEP_WIND);
        if value.contains(fan_control::WindBitmap::NATURAL_WIND) && !is_fan_mode && !sleep {
            let (_, horiz) = current_directions(&status);
            apply_directions(
                &mut status,
                VerticalDirection::Nice,
                horiz.unwrap_or(HorizontalDirection::Auto),
            )
            .map_err(set_err)?;
            apply_wind_speed(&mut status, WindSpeed::Auto).map_err(set_err)?;
        } else {
            // Clear Nice direction when NATURAL_WIND is off
            let (vert, horiz) = current_directions(&status);
            if vert == Some(VerticalDirection::Nice) {
                apply_directions(
                    &mut status,
                    VerticalDirection::Auto,
                    horiz.unwrap_or(HorizontalDirection::Auto),
                )
                .map_err(set_err)?;
            }
        }
        if !value.contains(fan_control::WindBitmap::SLEEP_WIND) {
            // Clear Silent speed when SLEEP_WIND is off
            let speed = current_wind_speed(&status).unwrap_or(WindSpeed::Auto);
            if speed == WindSpeed::Silent {
                apply_wind_speed(&mut status, WindSpeed::Auto).map_err(set_err)?;
            }
        }
        debug!("FanControl: wind_setting → {:?}", value);
        self.update(status)?;
        self.dataver.changed();
        Ok(())
    }

    fn handle_step(
        &self,
        _ctx: impl InvokeContext,
        _req: fan_control::StepRequest<'_>,
    ) -> Result<(), Error> {
        Err(ErrorCode::InvalidCommand.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_speed_to_setting_maps_silent_to_one() {
        assert_eq!(wind_speed_to_setting(WindSpeed::Silent), 1);
        assert_eq!(wind_speed_to_setting(WindSpeed::Lev1), 1);
    }

    #[test]
    fn wind_speed_to_setting_maps_auto_to_three() {
        assert_eq!(wind_speed_to_setting(WindSpeed::Auto), 3);
        assert_eq!(wind_speed_to_setting(WindSpeed::Lev3), 3);
    }

    #[test]
    fn wind_speed_to_setting_covers_each_level() {
        assert_eq!(wind_speed_to_setting(WindSpeed::Lev2), 2);
        assert_eq!(wind_speed_to_setting(WindSpeed::Lev4), 4);
        assert_eq!(wind_speed_to_setting(WindSpeed::Lev5), 5);
    }

    #[test]
    fn wind_speed_to_setting_unknown_returns_zero() {
        assert_eq!(wind_speed_to_setting(WindSpeed::Unknown), 0);
    }

    #[test]
    fn setting_to_wind_speed_maps_levels() {
        assert_eq!(setting_to_wind_speed(1), WindSpeed::Lev1);
        assert_eq!(setting_to_wind_speed(2), WindSpeed::Lev2);
        assert_eq!(setting_to_wind_speed(3), WindSpeed::Lev3);
        assert_eq!(setting_to_wind_speed(4), WindSpeed::Lev4);
        assert_eq!(setting_to_wind_speed(5), WindSpeed::Lev5);
    }

    #[test]
    fn setting_to_wind_speed_zero_falls_back_to_auto() {
        assert_eq!(setting_to_wind_speed(0), WindSpeed::Auto);
        assert_eq!(setting_to_wind_speed(99), WindSpeed::Auto);
    }

    #[test]
    fn setting_wind_speed_levels_roundtrip() {
        for level in 1..=5 {
            assert_eq!(wind_speed_to_setting(setting_to_wind_speed(level)), level);
        }
    }

    #[test]
    fn wind_speed_to_fan_mode_off_overrides_speed() {
        assert_eq!(
            wind_speed_to_fan_mode(WindSpeed::Lev5, true),
            fan_control::FanModeEnum::Off
        );
        assert_eq!(
            wind_speed_to_fan_mode(WindSpeed::Auto, true),
            fan_control::FanModeEnum::Off
        );
    }

    #[test]
    fn wind_speed_to_fan_mode_buckets_speeds() {
        assert_eq!(
            wind_speed_to_fan_mode(WindSpeed::Auto, false),
            fan_control::FanModeEnum::Auto
        );
        assert_eq!(
            wind_speed_to_fan_mode(WindSpeed::Silent, false),
            fan_control::FanModeEnum::Low
        );
        assert_eq!(
            wind_speed_to_fan_mode(WindSpeed::Lev1, false),
            fan_control::FanModeEnum::Low
        );
        assert_eq!(
            wind_speed_to_fan_mode(WindSpeed::Lev2, false),
            fan_control::FanModeEnum::Medium
        );
        assert_eq!(
            wind_speed_to_fan_mode(WindSpeed::Lev3, false),
            fan_control::FanModeEnum::Medium
        );
        assert_eq!(
            wind_speed_to_fan_mode(WindSpeed::Lev4, false),
            fan_control::FanModeEnum::High
        );
        assert_eq!(
            wind_speed_to_fan_mode(WindSpeed::Lev5, false),
            fan_control::FanModeEnum::High
        );
    }
}
