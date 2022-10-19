use serde::{
    de::{self, DeserializeSeed, Visitor},
    Deserialize, Serialize,
};

use crate::types::{Label, Value, ValueType};

/// A wrapper struct that associates a [`Value`] with its type,
/// allowing deserialization.
#[cfg_attr(feature = "derive-impls", derive(serde::Serialize))]
pub struct ValueWithType {
    #[cfg_attr(feature = "derive-impls", serde(rename = "type"))]
    pub ty: ValueType,
    pub value: Value,
}

enum ValueTypeFields {
    Type,
    Value,
}

struct HexVisitor;

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Value::Unknown => panic!("serialize unknown value"),
            Value::UnsignedByte(b) | Value::Percent(b) | Value::Unknown2(b) => {
                serializer.serialize_u8(*b)
            }
            Value::UnsignedShort(s) | Value::Unknown3(s) => serializer.serialize_u16(*s),
            Value::UnsignedInt(i) | Value::Unknown1(i) => serializer.serialize_u32(*i),
            Value::SignedByte(b) => serializer.serialize_i8(*b),
            Value::SignedShort(s) => serializer.serialize_i16(*s),
            Value::SignedInt(i) => serializer.serialize_i32(*i),
            Value::String(s) => serializer.serialize_str(s),
            Value::Float(f) => serializer.serialize_f32(*f),
            Value::HashRef(h) => {
                if serializer.is_human_readable() {
                    serializer.serialize_str(&format!("{:+}", Label::Hash(*h).to_string()))
                } else {
                    serializer.serialize_u32(*h)
                }
            }
        }
    }
}

impl ValueType {
    /// Deserializes the corresponding [`Value`] based on the type defined by self.
    pub fn deser_value<'de, D>(&self, deserializer: D) -> Result<Value, D::Error>
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
            Self::String => Value::String(String::deserialize(deserializer)?),
            Self::Float => Value::Float(f32::deserialize(deserializer)?),
            Self::HashRef => Value::HashRef(deserializer.deserialize_any(HexVisitor)?),
            Self::Percent => Value::Percent(u8::deserialize(deserializer)?),
            Self::Unknown1 => Value::Unknown1(u32::deserialize(deserializer)?),
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
        Ok(v.try_into()
            .map_err(|_| de::Error::invalid_value(de::Unexpected::Unsigned(v), &self))?)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(u32::from_str_radix(v, 16)
            .map_err(|_| de::Error::invalid_value(de::Unexpected::Str(v), &self))?)
    }
}

impl From<Value> for ValueWithType {
    fn from(v: Value) -> Self {
        Self {
            ty: ValueType::from(&v),
            value: v,
        }
    }
}

impl From<ValueWithType> for Value {
    fn from(vt: ValueWithType) -> Self {
        vt.value
    }
}

impl<'de> Deserialize<'de> for ValueWithType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValueWithTypeVisitor;

        impl<'de> Visitor<'de> for ValueWithTypeVisitor {
            type Value = ValueWithType;

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
                let ty = ValueType::try_from(ty).map_err(|_| {
                    serde::de::Error::invalid_value(
                        serde::de::Unexpected::Unsigned(ty as u64),
                        &self,
                    )
                })?;

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

        deserializer.deserialize_struct("ValueWithType", &["type", "value"], ValueWithTypeVisitor)
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
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        self.deser_value(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        serde::ValueWithType,
        types::{Value, ValueType},
    };

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
        let values: Vec<Value> = serde_json::from_value::<Vec<ValueWithType>>(json)
            .unwrap()
            .into_iter()
            .map(Into::into)
            .collect();
        assert_eq!(
            values,
            [
                Value::UnsignedByte(82),
                Value::String(String::from("Hello world")),
                Value::Float(1.01)
            ]
        );
    }

    #[test]
    #[should_panic]
    fn deser_overflow() {
        let json = serde_json::json!({ "type": ValueType::UnsignedByte, "value": 1000 });
        serde_json::from_value::<ValueWithType>(json).unwrap();
    }

    #[test]
    fn deser_external() {
        let ty = ValueType::Unknown1;
        let value = ty
            .deser_value(&mut serde_json::Deserializer::from_str("1024"))
            .unwrap();
        assert_eq!(value, Value::Unknown1(1024));
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
    }

    #[test]
    #[should_panic]
    fn json_deser_hash_overflow() {
        let ty = ValueType::HashRef;
        ty.deser_value(&mut serde_json::Deserializer::from_str("10000000000"))
            .unwrap();
    }
}
