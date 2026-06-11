//! Operating mode definitions.

use serde_repr::{Deserialize_repr, Serialize_repr};

/// HVAC operating mode.
#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum Mode {
    Fan = 0,
    Heating = 1,
    Cooling = 2,
    Auto = 3,
    Dehumidify = 5,

    Unknown = 255,
}

impl From<Mode> for f32 {
    fn from(val: Mode) -> Self {
        val as u8 as f32
    }
}

impl TryFrom<u8> for Mode {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Mode::Fan),
            1 => Ok(Mode::Heating),
            2 => Ok(Mode::Cooling),
            3 => Ok(Mode::Auto),
            5 => Ok(Mode::Dehumidify),
            255 => Ok(Mode::Unknown),
            _ => Err(()),
        }
    }
}
