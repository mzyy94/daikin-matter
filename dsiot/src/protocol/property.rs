use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::ops::RangeInclusive;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Serialize_repr, Deserialize_repr, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PropertyType {
    Tree = 1,
    ReadWrite = 2,
    ReadOnly = 3,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Property {
    Tree {
        #[serde(rename = "pn")]
        name: String,
        #[serde(skip_serializing, rename = "pt")]
        type_: PropertyType,
        #[serde(rename = "pch")]
        children: Vec<Property>,
    },
    Node(Item),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Item<T: Sized + DeserializeOwned + Into<f32> = f32> {
    #[serde(rename = "pn")]
    pub name: String,
    #[serde(skip_serializing, rename = "pt")]
    pub type_: PropertyType,
    #[serde(rename = "pv")]
    pub value: PropValue,
    #[serde(skip_serializing, rename = "md")]
    pub metadata: Metadata,
    #[serde(skip)]
    pub phantom: core::marker::PhantomData<fn() -> T>,
}

impl<T: Sized + DeserializeOwned + Into<f32>> PartialEq for Item<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Sized + DeserializeOwned + Into<f32>> core::fmt::Debug for Item<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Item {{ name: {:?}, ", self.name)?;
        match &self.metadata {
            Metadata::Binary(Binary::Step(..)) => {
                if let Some(v) = self.get_f32() {
                    write!(f, "value: {v:?}")
                } else {
                    write!(f, "value: {:?}", self.value)
                }
            }
            Metadata::Binary(Binary::String {}) => {
                write!(f, "value: {:?}", self.get_string())
            }
            Metadata::Binary(Binary::Enum(_)) => {
                if let PropValue::String(pv) = &self.value {
                    write!(f, "value: {:?}", hex2int(pv))
                } else {
                    write!(f, "value: {:?}", self.value)
                }
            }
            _ => {
                write!(f, "value: {:?}", self.value)
            }
        }?;
        write!(f, ", metadata: {:?} }}", self.metadata)
    }
}

fn hex2int(hex: &str) -> i32 {
    let Ok(bytes) = hex::decode(hex) else {
        return 0;
    };
    match bytes.len() {
        1 => i8::from_le_bytes([bytes[0]]) as i32,
        2 => i16::from_le_bytes([bytes[0], bytes[1]]) as i32,
        3 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]),
        4 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        _ => 0,
    }
}

impl Property {
    pub fn new_tree(name: &str) -> Property {
        Property::Tree {
            name: name.to_string(),
            type_: PropertyType::Tree,
            children: vec![],
        }
    }

    /// Returns the name of this property.
    pub fn name(&self) -> &str {
        match self {
            Property::Tree { name, .. } => name,
            Property::Node(item) => &item.name,
        }
    }

    pub fn find(&self, name: &str) -> Option<&Property> {
        match self {
            Property::Tree { children, .. } => children.iter().find(|p| p.name() == name),
            _ => None,
        }
    }

    pub fn find_mut(&mut self, name: &str) -> Option<&mut Property> {
        match self {
            Property::Tree { children, .. } => children.iter_mut().find(|p| p.name() == name),
            _ => None,
        }
    }

    pub fn push(&mut self, property: Property) -> Option<&mut Property> {
        match self {
            Property::Tree { children, .. } => {
                children.push(property);
                children.last_mut()
            }
            _ => None,
        }
    }
}

impl<T: Sized + DeserializeOwned + Into<f32>> Item<T> {
    pub fn get_f32(&self) -> Option<f32> {
        match self {
            Item {
                value: PropValue::String(pv),
                metadata: Metadata::Binary(Binary::Step(step)),
                ..
            } => {
                let value = hex2int(pv) as f32;
                let step = step.step();
                Some(value * step)
            }
            _ => None,
        }
    }

    pub fn set_value(&mut self, value: T) {
        match &self.metadata {
            Metadata::Integer => self.value = PropValue::Integer(value.into() as i32),
            Metadata::String => {} // String metadata does not support numeric value conversion
            Metadata::Binary(Binary::Step(step)) => {
                let value = (value.into() / step.step()) as i64;
                let bytes = value.to_le_bytes();
                self.value = PropValue::String(hex::encode(&bytes[..(step.max.len() / 2)]));
            }
            Metadata::Binary(Binary::Enum(e)) => {
                let value = value.into() as i64;
                let bytes = value.to_le_bytes();
                self.value = PropValue::String(hex::encode(&bytes[..(e.max.len() / 2)]));
            }
            _ => self.value = PropValue::Null,
        }
    }

    pub fn get_int(&self) -> Option<i32> {
        match self {
            Item {
                value: PropValue::Integer(pv),
                metadata: Metadata::Integer,
                ..
            } => Some(*pv),
            _ => None,
        }
    }

    pub fn get_enum(&self) -> Option<T> {
        match self {
            Item {
                value: PropValue::String(pv),
                metadata: Metadata::Binary(Binary::Enum(_)),
                ..
            } => {
                let value = hex2int(pv);
                serde_json::from_value(serde_json::Value::Number(serde_json::Number::from(value)))
                    .ok()
            }
            _ => None,
        }
    }

    pub fn get_string(&self) -> Option<String> {
        match self {
            Item {
                value: PropValue::String(pv),
                metadata: md,
                ..
            } => {
                if matches!(md, Metadata::String) {
                    Some(pv.clone())
                } else if matches!(md, Metadata::Binary(Binary::String {})) {
                    // Binary strings are stored in reverse byte order (little-endian UTF-8).
                    // Reverse the string to restore the original character order.
                    hex::decode(pv)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .map(|s| s.chars().rev().collect())
                        .map(|s: String| s.trim_end_matches('\0').to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(untagged)]
pub enum PropValue {
    String(String),
    Integer(i32),
    Null,
    StringList(Vec<String>),
    IntegerList(Vec<i32>),
    Object(serde_json::Value),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "pt")]
pub enum Metadata {
    #[serde(rename = "i")]
    Integer,
    #[serde(rename = "s")]
    String,
    #[serde(rename = "b")]
    Binary(Binary),
    #[serde(rename = "o")]
    Object,
    #[serde(rename = "l<s>")]
    StringList,
    #[serde(rename = "l<i>")]
    IntegerList,
    Undefined,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(untagged)]
pub enum Binary {
    Step(BinaryStep),
    Enum(BinaryEnum),
    String {},
}

/// Enum metadata with allowed values encoded as a bitmap.
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct BinaryEnum {
    /// Bitmap of allowed values. Each bit position corresponds to an enum value.
    /// Example: "2F00" allows values 0,1,2,3,5 (bits 0,1,2,3,5 are set).
    #[serde(rename = "mx")]
    pub max: String,
}

impl BinaryEnum {
    /// Returns the raw max value as a hex string.
    pub fn max(&self) -> &str {
        &self.max
    }

    /// Returns a list of allowed enum values based on the bitmap.
    ///
    /// The bitmap encoding works as follows:
    /// - Each byte represents 8 possible values
    /// - Byte index = value / 8
    /// - Bit position within byte = value % 8
    /// - If the bit is set, the value is allowed
    pub fn allowed_values(&self) -> Vec<u8> {
        let bytes = match hex::decode(&self.max) {
            Ok(b) => b,
            Err(_) => return vec![],
        };

        let mut values = Vec::new();
        for (byte_idx, &byte) in bytes.iter().enumerate() {
            for bit in 0..8 {
                if byte & (1 << bit) != 0 {
                    let value = (byte_idx * 8 + bit) as u8;
                    values.push(value);
                }
            }
        }
        values
    }

    /// Checks if a specific value is allowed by this enum.
    pub fn is_allowed(&self, value: u8) -> bool {
        let bytes = match hex::decode(&self.max) {
            Ok(b) => b,
            Err(_) => return false,
        };

        let byte_idx = (value / 8) as usize;
        let bit_pos = value % 8;

        if byte_idx >= bytes.len() {
            return false;
        }

        bytes[byte_idx] & (1 << bit_pos) != 0
    }
}

impl core::fmt::Debug for BinaryEnum {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "BinaryEnum {{ allowed: {:?} }}", self.allowed_values())
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct BinaryStep {
    #[serde(rename = "st")]
    pub step: u8,
    #[serde(rename = "mi")]
    pub min: String,
    #[serde(rename = "mx")]
    pub max: String,
}

impl BinaryStep {
    pub fn step(&self) -> f32 {
        let step = self.step;
        let step_base = f32::from(step & 0xf);
        let exp: i8 = (step & 0xf0) as i8 >> 4;
        let step_coefficient = libm::powf(10.0, exp as f32);
        step_base * step_coefficient
    }

    pub fn range(&self) -> RangeInclusive<f32> {
        let BinaryStep { min, max, step } = self;
        let step = if *step == 0 { 1.0 } else { self.step() };
        let min_value = hex2int(min) as f32 * step;
        let max_value = hex2int(max) as f32 * step;
        RangeInclusive::new(min_value, max_value)
    }
}

impl core::fmt::Debug for BinaryStep {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "BinaryStep {{ range: {:?}, step: {} }}",
            self.range(),
            self.step(),
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_hex2int() {
        let hex = "18".to_string();
        let result = hex2int(&hex);
        assert_eq!(result, 24);
        let hex = "eeff".to_string();
        let result = hex2int(&hex);
        assert_eq!(result, -18);
        let hex = "12345600".to_string();
        let result = hex2int(&hex);
        assert_eq!(result, 5649426);
    }

    #[test]
    fn get_f32() {
        let json = json!({
            "pn": "p_02",
            "pt": 2,
            "pv": "31",
            "md": {
                "pt": "b",
                "st": 245,
                "mi": "24",
                "mx": "40"
            }
        });
        let p: Item = serde_json::from_value(json).expect("Invalid JSON structure.");
        assert_eq!(p.get_f32(), Some(24.5));

        let json = json!({
            "pn": "root_entity_name",
            "pt": 3,
            "pv": "e_1002",
            "md": {
                "pt": "s"
            }
        });
        let p: Item = serde_json::from_value(json).expect("Invalid JSON structure.");
        assert_eq!(p.get_f32(), None);
    }

    #[test]
    fn get_string() {
        let json = json!({
            "pn": "root_entity_name",
            "pt": 3,
            "pv": "e_1002",
            "md": {
                "pt": "s"
            }
        });
        let p: Item = serde_json::from_value(json).expect("Invalid JSON structure.");
        assert_eq!(p.get_string(), Some("e_1002".into()));

        let json = json!({
            "pn": "data_model_code",
            "pt": 3,
            "pv": 26,
            "md": {
                "pt": "i"
            }
        });
        let p: Item = serde_json::from_value(json).expect("Invalid JSON structure.");
        assert_eq!(p.get_string(), None);
    }

    #[test]
    fn propvalue() {
        let json = json!({
            "pn": "p_01",
            "pt": 3,
            "pv": "2600",
            "md": {
                "pt": "b",
                "st": 245,
                "mi": "EEFF",
                "mx": "4E00"
            }
        });
        let item: Item = serde_json::from_value(json).expect("Invalid JSON structure.");
        let pv = item.value;
        let expect = PropValue::String("2600".into());

        assert_eq!(pv, expect);
    }

    #[test]
    fn find_push() {
        let mut tree = Property::new_tree("root");
        let _child = tree.push(Property::new_tree("child")).unwrap();
        assert!(matches!(tree.find("child"), Some(Property::Tree { .. })));
    }

    #[test]
    fn debug_display() {
        let json = json!({
            "pn": "e_A00D",
            "pt": 1,
            "pch": [
                {
                    "pn": "p_01",
                    "pt": 3,
                    "pv": "2600",
                    "md": {
                        "pt": "b",
                        "st": 245,
                        "mi": "EEFF",
                        "mx": "4E00"
                    }
                }
            ]
        });
        let p: Property = serde_json::from_value(json).expect("Invalid JSON structure.");

        assert_eq!(
            format!("{p:?}"),
            r#"Tree { name: "e_A00D", type_: Tree, children: [Node(Item { name: "p_01", value: 19.0, metadata: Binary(Step(BinaryStep { range: -9.0..=39.0, step: 0.5 })) })] }"#
        );
    }

    #[test]
    fn binary_enum_allowed_values() {
        // Mode enum: max="2F00" should allow 0,1,2,3,5 (Fan, Heating, Cooling, Auto, Dehumidify)
        let e = BinaryEnum {
            max: "2F00".to_string(),
        };
        assert_eq!(e.allowed_values(), vec![0, 1, 2, 3, 5]);
        assert!(e.is_allowed(0)); // Fan
        assert!(e.is_allowed(1)); // Heating
        assert!(e.is_allowed(2)); // Cooling
        assert!(e.is_allowed(3)); // Auto
        assert!(!e.is_allowed(4)); // Not allowed
        assert!(e.is_allowed(5)); // Dehumidify

        // WindSpeed: max="F80C" should allow 3,4,5,6,7,10,11
        let e = BinaryEnum {
            max: "F80C".to_string(),
        };
        assert_eq!(e.allowed_values(), vec![3, 4, 5, 6, 7, 10, 11]);
        assert!(!e.is_allowed(0));
        assert!(e.is_allowed(3)); // Lev1
        assert!(e.is_allowed(10)); // Auto
        assert!(e.is_allowed(11)); // Silent

        // AutoModeWindSpeed: max="000C" should allow only 10,11 (Auto, Silent)
        let e = BinaryEnum {
            max: "000C".to_string(),
        };
        assert_eq!(e.allowed_values(), vec![10, 11]);
        assert!(!e.is_allowed(3));
        assert!(e.is_allowed(10)); // Auto
        assert!(e.is_allowed(11)); // Silent
    }

    #[test]
    fn binary_enum_vertical_direction() {
        // VerticalDirection: max="3F808100"
        // Should allow: 1,2,3,4,5 (positions), 15 (Swing), 16 (Auto), 23 (Nice)
        let e = BinaryEnum {
            max: "3F808100".to_string(),
        };
        let allowed = e.allowed_values();
        assert!(allowed.contains(&1)); // TopMost
        assert!(allowed.contains(&2)); // Top
        assert!(allowed.contains(&3)); // Center
        assert!(allowed.contains(&4)); // Bottom
        assert!(allowed.contains(&5)); // BottomMost
        assert!(allowed.contains(&15)); // Swing (0x0F)
        assert!(allowed.contains(&16)); // Auto (0x10)
        assert!(allowed.contains(&23)); // Nice (0x17)
        assert!(!allowed.contains(&6)); // Not allowed
    }
}
