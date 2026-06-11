use super::property::{Item, Property};
use super::request::{DaikinRequest, Request};
use super::response::DaikinResponse;
use crate::types::{AutoModeWindSpeed, HorizontalDirection, Mode, VerticalDirection, WindSpeed};
use alloc::vec;

/// Sensor readings from the device (read-only values).
#[derive(Clone, Debug, PartialEq)]
pub struct SensorReadings {
    /// Indoor temperature in Celsius.
    pub temperature: Item<f32>,
    /// Indoor humidity percentage.
    pub humidity: Item<f32>,
    /// Outdoor temperature in Celsius.
    pub outdoor_temperature: Item<f32>,
}

/// Temperature target settings for each mode.
#[derive(Clone, Debug, PartialEq)]
pub struct TemperatureSettings {
    /// Target temperature for cooling mode.
    pub cooling: Item<f32>,
    /// Target temperature for heating mode.
    pub heating: Item<f32>,
    /// Temperature offset for auto mode (-5 to +5).
    pub automatic: Item<f32>,
}

/// Wind/airflow settings for a specific mode (cooling, heating, dehumidify).
#[derive(Clone, Debug, PartialEq)]
pub struct ModeWindSettings {
    /// Fan speed setting.
    pub speed: Item<WindSpeed>,
    /// Vertical air direction.
    pub vertical_direction: Item<VerticalDirection>,
    /// Horizontal air direction.
    pub horizontal_direction: Item<HorizontalDirection>,
}

/// Wind/airflow settings for auto mode (limited speed options).
#[derive(Clone, Debug, PartialEq)]
pub struct AutoModeWindSettings {
    /// Fan speed setting (Auto or Silent only).
    pub speed: Item<AutoModeWindSpeed>,
    /// Vertical air direction.
    pub vertical_direction: Item<VerticalDirection>,
    /// Horizontal air direction.
    pub horizontal_direction: Item<HorizontalDirection>,
}

/// Wind/airflow control settings per operating mode.
#[derive(Clone, Debug, PartialEq)]
pub struct WindSettings {
    /// Wind settings for cooling mode.
    pub cooling: ModeWindSettings,
    /// Wind settings for heating mode.
    pub heating: ModeWindSettings,
    /// Wind settings for fan mode.
    pub fan: ModeWindSettings,
    /// Wind settings for dehumidify mode.
    pub dehumidify: ModeWindSettings,
    /// Wind settings for auto mode.
    pub auto: AutoModeWindSettings,
}

/// Complete device status containing all readable and writable properties.
#[derive(Clone, Debug, PartialEq)]
pub struct DaikinStatus {
    /// Power state (0.0 = off, 1.0 = on).
    pub power: Item<f32>,
    /// Operating mode.
    pub mode: Item<Mode>,
    /// Sensor readings (temperature, humidity).
    pub sensors: SensorReadings,
    /// Temperature settings for each mode.
    pub temperature: TemperatureSettings,
    /// Wind/airflow settings.
    pub wind: WindSettings,
    /// Instantaneous power consumption in watts (requires en_ipower).
    pub power_consumption: Item<f32>,
}

impl From<DaikinResponse> for DaikinStatus {
    fn from(response: DaikinResponse) -> Self {
        DaikinStatus {
            power: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_A002.p_01),
            mode: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_01),
            sensors: SensorReadings {
                temperature: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_A00B.p_01),
                humidity: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_A00B.p_02),
                outdoor_temperature: get_prop!(response."/dsiot/edge/adr_0200.dgc_status".e_1003.e_A00D.p_01),
            },
            temperature: TemperatureSettings {
                cooling: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_02),
                heating: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_03),
                automatic: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_1F),
            },
            wind: WindSettings {
                cooling: ModeWindSettings {
                    speed: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_09),
                    vertical_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_05),
                    horizontal_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_06),
                },
                heating: ModeWindSettings {
                    speed: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_0A),
                    vertical_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_07),
                    horizontal_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_08),
                },
                fan: ModeWindSettings {
                    speed: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_28),
                    vertical_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_24),
                    horizontal_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_25),
                },
                dehumidify: ModeWindSettings {
                    speed: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_27),
                    vertical_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_22),
                    horizontal_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_23),
                },
                auto: AutoModeWindSettings {
                    speed: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_26),
                    vertical_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_20),
                    horizontal_direction: get_prop!(response."/dsiot/edge/adr_0100.dgc_status".e_1002.e_3001.p_21),
                },
            },
            power_consumption: get_prop!(response."/dsiot/edge/adr_0200.dgc_status".e_1003.e_A005.p_01),
        }
    }
}

impl From<DaikinStatus> for DaikinRequest {
    fn from(status: DaikinStatus) -> Self {
        let mut prop = Property::new_tree("dgc_status");

        set_child_prop!({ prop }.e_1002.e_A002.p_01 = status.power);
        set_child_prop!({ prop }.e_1002.e_3001.p_01 = status.mode);
        set_child_prop!({ prop }.e_1002.e_3001.p_02 = status.temperature.cooling);
        set_child_prop!({ prop }.e_1002.e_3001.p_03 = status.temperature.heating);
        set_child_prop!({ prop }.e_1002.e_3001.p_1F = status.temperature.automatic);

        // Wind settings per mode: (speed, vertical, horizontal) property names.
        macro_rules! set_wind {
            ($w:expr, $sp:ident, $v:ident, $h:ident) => {{
                set_child_prop!({ prop }.e_1002.e_3001.$sp = $w.speed);
                set_child_prop!({ prop }.e_1002.e_3001.$v = $w.vertical_direction);
                set_child_prop!({ prop }.e_1002.e_3001.$h = $w.horizontal_direction);
            }};
        }
        set_wind!(status.wind.cooling, p_09, p_05, p_06);
        set_wind!(status.wind.heating, p_0A, p_07, p_08);
        set_wind!(status.wind.fan, p_28, p_24, p_25);
        set_wind!(status.wind.dehumidify, p_27, p_22, p_23);
        set_wind!(status.wind.auto, p_26, p_20, p_21);

        DaikinRequest {
            requests: vec![Request {
                op: 3,
                pc: prop,
                to: "/dsiot/edge/adr_0100.dgc_status".into(),
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getter() {
        let res: DaikinResponse = serde_json::from_str(include_str!("../fixtures/status.json"))
            .expect("Invalid JSON file.");
        let status: DaikinStatus = res.into();

        assert_eq!(status.power.get_f32(), Some(0.0));
        assert_eq!(status.mode.get_enum(), Some(Mode::Cooling));

        // Sensor readings
        assert_eq!(status.sensors.temperature.get_f32(), Some(20.0));
        assert_eq!(status.sensors.humidity.get_f32(), Some(50.0));
        assert_eq!(status.sensors.outdoor_temperature.get_f32(), Some(19.0));

        // Temperature settings
        assert_eq!(status.temperature.cooling.get_f32(), Some(24.5));
        assert_eq!(status.temperature.heating.get_f32(), Some(25.0));
        assert_eq!(status.temperature.automatic.get_f32(), Some(0.0));

        // Cooling wind settings
        assert_eq!(status.wind.cooling.speed.get_enum(), Some(WindSpeed::Auto));
        assert_eq!(
            status.wind.cooling.vertical_direction.get_enum(),
            Some(VerticalDirection::Auto)
        );
        assert_eq!(
            status.wind.cooling.horizontal_direction.get_enum(),
            Some(HorizontalDirection::Auto)
        );

        // Heating wind settings
        assert_eq!(status.wind.heating.speed.get_enum(), Some(WindSpeed::Auto));

        // Auto mode wind settings
        assert_eq!(
            status.wind.auto.speed.get_enum(),
            Some(AutoModeWindSpeed::Auto)
        );

        // Power consumption
        assert_eq!(status.power_consumption.get_f32(), Some(0.0));
    }

    #[test]
    fn setter() {
        let res: DaikinResponse = serde_json::from_str(include_str!("../fixtures/status.json"))
            .expect("Invalid JSON file.");
        let mut status: DaikinStatus = res.into();

        status.power.set_value(1.0).unwrap();
        status.mode.set_value(Mode::Cooling).unwrap();
        status.temperature.cooling.set_value(24.5).unwrap();
        status.temperature.heating.set_value(25.0).unwrap();
        status.temperature.automatic.set_value(0.0).unwrap();

        // Cooling wind settings
        status
            .wind
            .cooling
            .speed
            .set_value(WindSpeed::Lev4)
            .unwrap();
        status
            .wind
            .cooling
            .vertical_direction
            .set_value(VerticalDirection::BottomMost)
            .unwrap();
        status
            .wind
            .cooling
            .horizontal_direction
            .set_value(HorizontalDirection::RightCenter)
            .unwrap();

        // Auto mode wind settings
        status
            .wind
            .auto
            .speed
            .set_value(AutoModeWindSpeed::Silent)
            .unwrap();

        let req: DaikinRequest = status.into();
        let json = serde_json::to_string(&req).unwrap();

        assert_eq!(req.requests.len(), 1);
        assert_eq!(req.requests[0].op, 3);
        assert_eq!(req.requests[0].to, "/dsiot/edge/adr_0100.dgc_status");

        // Every property set above is serialized into the request tree.
        assert!(json.contains(r#""pn":"e_A002""#)); // power group
        assert!(json.contains(r#""pn":"e_3001""#)); // mode/temperature/wind group
        assert!(json.contains(r#""pn":"p_02""#)); // cooling setpoint
        assert!(json.contains(r#""pn":"p_09""#)); // cooling wind speed
        assert!(json.contains(r#""pn":"p_26""#)); // auto wind speed
    }
}
