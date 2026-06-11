//! Daikin-specific protocol implementation.

#[macro_use]
mod macros;
pub mod info;
pub mod property;
pub mod request;
pub mod response;
pub mod status;

pub use info::DaikinInfo;
pub use property::{
    Binary, BinaryEnum, BinaryStep, Item, Metadata, PropTag, PropValue, Property, SetValueError,
};
pub use request::DaikinRequest;
pub use response::DaikinResponse;
pub use status::{
    AutoModeWindSettings, DaikinStatus, ModeWindSettings, SensorReadings, TemperatureSettings,
    WindSettings,
};
