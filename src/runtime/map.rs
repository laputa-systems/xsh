//! Runtime conversions for prepared scalar map keys.

use super::value::{DurationValue, PathValue, Value};
use crate::map_key::{MapKey, MapKeyRef};

impl MapKey {
    pub fn into_value(self) -> Value {
        match self {
            Self::Str(value) => Value::Str(value),
            Self::Int(value) => Value::Int(value),
            Self::Bool(value) => Value::Bool(value),
            Self::Bytes(value) => Value::Bytes(value.as_ref().to_vec()),
            Self::Path(value) => Value::Path(PathValue {
                bytes: value.as_ref().to_vec(),
            }),
            Self::Duration(millis) => Value::Duration(DurationValue { millis }),
        }
    }
}

impl<'a> MapKeyRef<'a> {
    pub fn from_value(value: &'a Value) -> Option<Self> {
        Some(match value {
            Value::Str(value) => Self::Str(value),
            Value::Int(value) => Self::Int(*value),
            Value::Bool(value) => Self::Bool(*value),
            Value::Bytes(value) => Self::Bytes(value),
            Value::Path(value) => Self::Path(&value.bytes),
            Value::Duration(value) => Self::Duration(value.millis),
            _ => return None,
        })
    }
}
