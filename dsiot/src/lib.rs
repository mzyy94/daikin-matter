//! DSIOT - Daikin Smart IoT Protocol Library
//!
//! This crate provides protocol-agnostic abstractions for HVAC control,
//! with specific implementations for Daikin devices.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod constraints;
pub mod mapping;
pub mod protocol;
pub mod state;
pub mod temperature;
pub mod types;

pub use constraints::ValueConstraints;
pub use state::{DeviceState, PowerState, StateTransition, StateTransitionError};
pub use temperature::{TemperatureError, TemperatureTarget};
pub use types::{AutoModeWindSpeed, HorizontalDirection, Mode, VerticalDirection, WindSpeed};

pub use protocol::{
    AutoModeWindSettings, Binary, BinaryEnum, BinaryStep, DaikinInfo, DaikinRequest,
    DaikinResponse, DaikinStatus, Item, Metadata, ModeWindSettings, PropValue, Property,
    SensorReadings, TemperatureSettings, WindSettings,
};
