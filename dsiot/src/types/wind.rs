//! Wind/airflow control type definitions.

use serde_repr::{Deserialize_repr, Serialize_repr};

/// Fan speed setting.
#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum WindSpeed {
    Silent = 0x0B,
    Lev1 = 0x03,
    Lev2 = 0x04,
    Lev3 = 0x05,
    Lev4 = 0x06,
    Lev5 = 0x07,
    Auto = 0x0A,

    Unknown = 0xFF,
}

impl From<WindSpeed> for f32 {
    fn from(val: WindSpeed) -> Self {
        val as u8 as f32
    }
}

impl TryFrom<u8> for WindSpeed {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x0B => Ok(WindSpeed::Silent),
            0x03 => Ok(WindSpeed::Lev1),
            0x04 => Ok(WindSpeed::Lev2),
            0x05 => Ok(WindSpeed::Lev3),
            0x06 => Ok(WindSpeed::Lev4),
            0x07 => Ok(WindSpeed::Lev5),
            0x0A => Ok(WindSpeed::Auto),
            0xFF => Ok(WindSpeed::Unknown),
            _ => Err(()),
        }
    }
}

/// Fan speed setting for auto mode.
#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum AutoModeWindSpeed {
    Silent = 0x0B,
    Auto = 0x0A,

    Unknown = 0xFF,
}

impl From<AutoModeWindSpeed> for f32 {
    fn from(val: AutoModeWindSpeed) -> Self {
        val as u8 as f32
    }
}

impl TryFrom<u8> for AutoModeWindSpeed {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x0B => Ok(AutoModeWindSpeed::Silent),
            0x0A => Ok(AutoModeWindSpeed::Auto),
            0xFF => Ok(AutoModeWindSpeed::Unknown),
            _ => Err(()),
        }
    }
}

/// Vertical air direction.
#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum VerticalDirection {
    TopMost = 0x01,
    Top = 0x02,
    Center = 0x03,
    Bottom = 0x04,
    BottomMost = 0x05,

    Swing = 0x0F,
    Auto = 0x10,

    Nice = 0x17,

    Unknown = 0xFF,
}

impl From<VerticalDirection> for f32 {
    fn from(val: VerticalDirection) -> Self {
        val as u8 as f32
    }
}

impl TryFrom<u8> for VerticalDirection {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x01 => Ok(VerticalDirection::TopMost),
            0x02 => Ok(VerticalDirection::Top),
            0x03 => Ok(VerticalDirection::Center),
            0x04 => Ok(VerticalDirection::Bottom),
            0x05 => Ok(VerticalDirection::BottomMost),
            0x0F => Ok(VerticalDirection::Swing),
            0x10 => Ok(VerticalDirection::Auto),
            0x17 => Ok(VerticalDirection::Nice),
            0xFF => Ok(VerticalDirection::Unknown),
            _ => Err(()),
        }
    }
}

/// Horizontal air direction.
#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum HorizontalDirection {
    LeftMost = 0x02,
    Left = 0x03,
    LeftCenter = 0x04,
    Center = 0x05,
    RightCenter = 0x06,
    Right = 0x07,
    RightMost = 0x08,

    Swing = 0x0F,
    Auto = 0x10,

    Unknown = 0xFF,
}

impl From<HorizontalDirection> for f32 {
    fn from(val: HorizontalDirection) -> Self {
        val as u8 as f32
    }
}

impl TryFrom<u8> for HorizontalDirection {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x02 => Ok(HorizontalDirection::LeftMost),
            0x03 => Ok(HorizontalDirection::Left),
            0x04 => Ok(HorizontalDirection::LeftCenter),
            0x05 => Ok(HorizontalDirection::Center),
            0x06 => Ok(HorizontalDirection::RightCenter),
            0x07 => Ok(HorizontalDirection::Right),
            0x08 => Ok(HorizontalDirection::RightMost),
            0x0F => Ok(HorizontalDirection::Swing),
            0x10 => Ok(HorizontalDirection::Auto),
            0xFF => Ok(HorizontalDirection::Unknown),
            _ => Err(()),
        }
    }
}
