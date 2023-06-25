use crate::legacy::float::BdatReal;
use crate::ColumnDef;
use serde::de::value::MapAccessDeserializer;
use serde::de::MapAccess;
use serde::ser::SerializeMap;
use serde::{
    de::{self, DeserializeSeed, Visitor},
    ser, Deserialize, Deserializer, Serialize, Serializer,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::marker::PhantomData;

use crate::types::{Cell, Label, Value, ValueType};

/// A wrapper struct that associates a [`Value`] with its type,
/// allowing deserialization.
#[derive(serde::Serialize)]
pub struct ValueWithType<'b> {
    #[serde(rename = "type")]
    pub ty: ValueType,
    pub value: Value<'b>,
}

/// Wraps a cell with its column to allow for custom serialization.
pub struct SerializeCell<'a, 'b, 't> {
    column: &'a ColumnDef,
    cell: &'b Cell<'t>,
}

enum ValueTypeFields {
    Type,
    Value,
}

struct HexVisitor;

/// An implementation of [`DeserializeSeed`] for [`Cell`]s.
pub struct CellSeed<'a>(&'a ColumnDef);

impl ColumnDef {
    pub fn as_cell_seed(&self) -> CellSeed {
        CellSeed(self)
    }

    pub fn cell_serializer<'a, 'b, 't>(&'a self, cell: &'b Cell<'t>) -> SerializeCell<'a, 'b, 't> {
        SerializeCell { column: self, cell }
    }
}

impl<'a, 'b, 't> Serialize for SerializeCell<'a, 'b, 't> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.cell {
            Cell::Single(v) => v.serialize(serializer),
            Cell::List(values) => values.serialize(serializer),
            Cell::Flags(flag_values) => {
                let keys = self.column.flags();
                let mut map = serializer.serialize_map(Some(flag_values.len()))?;
                for (i, val) in flag_values.iter().enumerate() {
                    let name = keys.get(i).map(|f| &f.label).ok_or_else(|| {
                        ser::Error::custom(format!("no name for flag at index {i}"))
                    })?;
                    map.serialize_entry(&name, val)?;
                }
                map.end()
            }
        }
    }
}

impl<'b> Serialize for Value<'b> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Value::Unknown => panic!("serialize unknown value"),
            Value::UnsignedByte(b) | Value::Percent(b) | Value::Unknown2(b) => {
                serializer.serialize_u8(*b)
            }
            Value::UnsignedShort(s) | Value::Unknown3(s) => serializer.serialize_u16(*s),
            Value::UnsignedInt(i) => serializer.serialize_u32(*i),
            Value::SignedByte(b) => serializer.serialize_i8(*b),
            Value::SignedShort(s) => serializer.serialize_i16(*s),
            Value::SignedInt(i) => serializer.serialize_i32(*i),
            Value::String(s) | Value::DebugString(s) => serializer.serialize_str(s),
            Value::Float(f) => serializer.serialize_f32((*f).into()),
            Value::HashRef(h) => {
                if serializer.is_human_readable() {
                    serializer.serialize_str(&format!("{}", Label::Hash(*h)))
                } else {
                    serializer.serialize_u32(*h)
                }
            }
        }
    }
}

impl ValueType {
    /// Deserializes the corresponding [`Value`] based on the type defined by self.
    pub fn deser_value<'de, D>(&self, deserializer: D) -> Result<Value<'de>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match self {
            Self::Unknown => Value::Unknown,
            Self::UnsignedInt => Value::UnsignedInt(u32::deserialize(deserializer)?),
            Self::UnsignedShort => Value::UnsignedShort(u16::deserialize(deserializer)?),
            Self::UnsignedByte => Value::UnsignedByte(u8::deserialize(deserializer)?),
            Self::SignedInt => Value::SignedInt(i32::deserialize(deserializer)?),
            Self::SignedShort => Value::SignedShort(i16::deserialize(deserializer)?),
            Self::SignedByte => Value::SignedByte(i8::deserialize(deserializer)?),
            Self::String => Value::String(Cow::deserialize(deserializer)?),
            Self::Float => Value::Float(BdatReal::Unknown(f32::deserialize(deserializer)?)),
            Self::HashRef => Value::HashRef(deserializer.deserialize_any(HexVisitor)?),
            Self::Percent => Value::Percent(u8::deserialize(deserializer)?),
            Self::DebugString => Value::DebugString(Cow::deserialize(deserializer)?),
            Self::Unknown2 => Value::Unknown2(u8::deserialize(deserializer)?),
            Self::Unknown3 => Value::Unknown3(u16::deserialize(deserializer)?),
        })
    }
}

impl<'de> Visitor<'de> for HexVisitor {
    type Value = u32;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("number or hex string")
    }

    fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(v)
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        v.try_into()
            .map_err(|_| de::Error::invalid_value(de::Unexpected::Unsigned(v), &self))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v.len() {
            10 if v.as_bytes()[0] == b'<' => u32::from_str_radix(&v[1..=8], 16), // <XXXXXXXX>
            _ => u32::from_str_radix(v, 16),
        }
        .map_err(|_| de::Error::invalid_value(de::Unexpected::Str(v), &self))
    }
}

impl<'b> From<Value<'b>> for ValueWithType<'b> {
    fn from(v: Value<'b>) -> Self {
        Self {
            ty: ValueType::from(&v),
            value: v,
        }
    }
}

impl<'b> From<ValueWithType<'b>> for Value<'b> {
    fn from(vt: ValueWithType<'b>) -> Self {
        vt.value
    }
}

impl<'de: 'tb, 'tb> Deserialize<'de> for ValueWithType<'tb> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValueWithTypeVisitor<'tb> {
            _marker: PhantomData<&'tb ()>,
        }

        impl<'de: 'tb, 'tb> Visitor<'de> for ValueWithTypeVisitor<'tb> {
            type Value = ValueWithType<'tb>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct ValueWithType")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let ty: ValueType = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;

                let value = seq
                    .next_element_seed(ty)?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;

                Ok(ValueWithType { ty, value })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut ty = None;
                let mut val = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        ValueTypeFields::Type => {
                            if ty.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            let deser_ty: u8 = map.next_value()?;
                            ty = Some(ValueType::try_from(deser_ty).map_err(|_| {
                                serde::de::Error::invalid_value(
                                    serde::de::Unexpected::Unsigned(deser_ty as u64),
                                    &self,
                                )
                            })?);
                        }
                        ValueTypeFields::Value => {
                            if val.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            val = Some(map.next_value_seed(
                                ty.ok_or_else(|| de::Error::missing_field("type"))?,
                            )?);
                        }
                    }
                }

                let value = val.ok_or_else(|| de::Error::missing_field("value"))?;

                Ok(ValueWithType {
                    ty: ty.unwrap(),
                    value,
                })
            }
        }

        deserializer.deserialize_struct(
            "ValueWithType",
            &["type", "value"],
            ValueWithTypeVisitor {
                _marker: PhantomData,
            },
        )
    }
}

impl<'de> Deserialize<'de> for ValueTypeFields {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct FieldVisitor;

        impl<'de> Visitor<'de> for FieldVisitor {
            type Value = ValueTypeFields;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("`type` or `value`")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "type" => Ok(ValueTypeFields::Type),
                    "value" => Ok(ValueTypeFields::Value),
                    f => Err(serde::de::Error::unknown_field(f, &["type", "value"])),
                }
            }
        }

        deserializer.deserialize_identifier(FieldVisitor)
    }
}

impl<'de> DeserializeSeed<'de> for ValueType {
    type Value = Value<'de>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        self.deser_value(deserializer)
    }
}

impl<'a, 'de> DeserializeSeed<'de> for CellSeed<'a> {
    type Value = Cell<'de>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct CellVisitor<'a>(&'a ColumnDef);

        impl<'a, 'de> Visitor<'de> for CellVisitor<'a> {
            type Value = Cell<'de>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Value, sequence of Values, or map with numeric values")
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                // Cell::Flags
                let map = HashMap::<String, u32>::deserialize(MapAccessDeserializer::new(map))?;
                let values = self
                    .0
                    .flags
                    .iter()
                    .filter_map(|f| map.get(&f.label))
                    .copied()
                    .collect();
                Ok(Cell::Flags(values))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                // Cell::List
                let mut values = Vec::with_capacity(seq.size_hint().unwrap_or_default());
                while let Some(v) = seq.next_element_seed(self.0.value_type)? {
                    values.push(v);
                }
                Ok(Cell::List(values))
            }
        }

        // Hacky way to mimic untagged enum deserialization
        let value = serde_value::Value::deserialize(deserializer)?;
        value
            .clone()
            .deserialize_any(CellVisitor(self.0))
            .or_else(|_| {
                Ok(Cell::Single(
                    self.0
                        .value_type
                        .deserialize(value)
                        .map_err(|e| e.into_error())?,
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        serde::ValueWithType,
        types::{Cell, Value, ValueType},
        ColumnDef, FlagDef, Label,
    };
    use serde::{de::DeserializeSeed, Deserialize};

    macro_rules! col {
        ($ty:expr) => {
            $crate::ColumnDef::new($ty, $crate::Label::Hash(0))
        };
    }

    #[test]
    fn json_single() {
        let value = Value::Percent(10);
        assert_eq!(serde_json::to_string(&value).unwrap(), r#"10"#);
    }

    #[test]
    fn json_with_type() {
        let value = Value::Percent(55);
        let type_num = ValueType::from(&value) as u8;
        assert_eq!(
            serde_json::to_string(&ValueWithType::from(value)).unwrap(),
            format!("{{\"type\":{type_num},\"value\":55}}")
        );
    }

    #[test]
    fn json_deser() {
        let json = serde_json::json!([
            {
                "type": ValueType::UnsignedByte,
                "value": 82
            },
            {
                "type": ValueType::String,
                "value": "Hello world"
            },
            {
                "type": ValueType::Float,
                "value": 1.01
            }
        ]);
        let values: Vec<Value> = Vec::<ValueWithType>::deserialize(json)
            .unwrap()
            .into_iter()
            .map(Into::into)
            .collect();
        assert_eq!(
            values,
            [
                Value::UnsignedByte(82),
                Value::String(String::from("Hello world").into()),
                Value::Float(1.01.into())
            ]
        );
    }

    #[test]
    #[should_panic]
    fn deser_overflow() {
        let json = serde_json::json!({ "type": ValueType::UnsignedByte, "value": 1000 });
        ValueWithType::deserialize(json).unwrap();
    }

    #[test]
    fn deser_external() {
        let ty = ValueType::Unknown3;
        let value = ty
            .deser_value(&mut serde_json::Deserializer::from_str("1024"))
            .unwrap();
        assert_eq!(value, Value::Unknown3(1024));
    }

    #[test]
    fn deser_hash() {
        let ty = ValueType::HashRef;

        assert_eq!(
            ty.deser_value(&mut serde_json::Deserializer::from_str("1"))
                .unwrap(),
            Value::HashRef(1)
        );
        assert_eq!(
            ty.deser_value(&mut serde_json::Deserializer::from_str("\"FFFFFFFF\""))
                .unwrap(),
            Value::HashRef(u32::MAX)
        );
        assert_eq!(
            ty.deser_value(&mut serde_json::Deserializer::from_str("\"<01ABCDEF>\""))
                .unwrap(),
            Value::HashRef(0x01abcdef)
        );
    }

    #[test]
    #[should_panic]
    fn json_deser_hash_overflow() {
        let ty = ValueType::HashRef;
        ty.deser_value(&mut serde_json::Deserializer::from_str("10000000000"))
            .unwrap();
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn deser_cell() {
        assert_eq!(
            col!(ValueType::UnsignedInt)
                .as_cell_seed()
                .deserialize(&mut serde_json::Deserializer::from_str(
                    "[1, 2, 3, 4, 5, 6]"
                ))
                .unwrap(),
            Cell::List(vec![
                Value::UnsignedInt(1),
                Value::UnsignedInt(2),
                Value::UnsignedInt(3),
                Value::UnsignedInt(4),
                Value::UnsignedInt(5),
                Value::UnsignedInt(6),
            ])
        );

        assert_eq!(
            col!(ValueType::Float)
                .as_cell_seed()
                .deserialize(&mut serde_json::Deserializer::from_str("3.14"))
                .unwrap(),
            Cell::Single(Value::Float(3.14.into()))
        );
    }

    #[test]
    fn serde_flags() {
        let column = ColumnDef {
            label: Label::Hash(0),
            value_type: ValueType::UnsignedInt,
            count: 1,
            offset: 0, // TODO better way of initializing the entire thing
            flags: vec![
                FlagDef {
                    label: "Flag1".to_string(),
                    mask: 1 << 2,
                    flag_index: 0,
                },
                FlagDef {
                    label: "Flag2".to_string(),
                    mask: 1 << 3,
                    flag_index: 1,
                },
                FlagDef {
                    label: "Flag3".to_string(),
                    mask: 1 << 4,
                    flag_index: 2,
                },
            ],
        };

        assert_eq!(
            r#"{"Flag1":1,"Flag2":3,"Flag3":4}"#,
            serde_json::to_string(&column.cell_serializer(&Cell::Flags(vec![1, 3, 4]))).unwrap()
        );

        assert_eq!(
            Cell::Flags(vec![1, 3, 4]),
            column
                .as_cell_seed()
                .deserialize(&mut serde_json::Deserializer::from_str(
                    r#"{"Flag1":1,"Flag2":3,"Flag3":4}"#
                ))
                .unwrap()
        );
    }
}
