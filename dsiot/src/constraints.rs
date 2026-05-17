//! Constraint extraction from property metadata.
//!
//! This module provides utilities to extract value constraints (min, max, step)
//! from Daikin's binary protocol metadata, enabling protocol-agnostic
//! constraint propagation to smart home platforms.

use crate::protocol::property::{Binary, Item, Metadata};
use serde::de::DeserializeOwned;

/// Value constraints for a numeric property.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueConstraints {
    /// Minimum allowed value.
    pub min: f32,
    /// Maximum allowed value.
    pub max: f32,
    /// Step increment between valid values.
    pub step: f32,
}

impl ValueConstraints {
    /// Create new constraints with specified values.
    pub fn new(min: f32, max: f32, step: f32) -> Self {
        Self { min, max, step }
    }

    /// Extract constraints from property metadata.
    ///
    /// Returns `Some(ValueConstraints)` if the metadata contains step information,
    /// `None` otherwise.
    pub fn from_metadata(metadata: &Metadata) -> Option<Self> {
        match metadata {
            Metadata::Binary(Binary::Step(step)) => {
                let range = step.range();
                Some(Self {
                    min: *range.start(),
                    max: *range.end(),
                    step: step.step(),
                })
            }
            _ => None,
        }
    }

    /// Extract constraints from an Item.
    pub fn from_item<T: Sized + DeserializeOwned + Into<f32>>(item: &Item<T>) -> Option<Self> {
        Self::from_metadata(&item.metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::property::BinaryStep;

    #[test]
    fn test_from_metadata_with_step() {
        let step = BinaryStep {
            step: 0xF5, // 5 * 10^-1 = 0.5
            min: "24".to_string(),
            max: "40".to_string(),
        };
        let metadata = Metadata::Binary(Binary::Step(step));

        let constraints = ValueConstraints::from_metadata(&metadata).unwrap();
        assert_eq!(constraints.min, 18.0);
        assert_eq!(constraints.max, 32.0);
        assert_eq!(constraints.step, 0.5);
    }

    #[test]
    fn test_from_metadata_without_step() {
        use crate::protocol::property::BinaryEnum;
        let metadata = Metadata::Binary(Binary::Enum(BinaryEnum {
            max: "FF".to_string(),
        }));

        assert!(ValueConstraints::from_metadata(&metadata).is_none());
    }

    #[test]
    fn test_from_metadata_integer() {
        let metadata = Metadata::Integer;
        assert!(ValueConstraints::from_metadata(&metadata).is_none());
    }
}
