use super::*;
use serde::{
    Deserialize,
    de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use std::{fmt, io::Read};
struct Seed<'a>(&'a Select);
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        d: D,
    ) -> std::result::Result<Value, D::Error> {
        if matches!(self.0, Select::All | Select::Raw) {
            Value::deserialize(d)
        } else {
            d.deserialize_any(self)
        }
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("source control metadata")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> std::result::Result<Value, M::Error> {
        let mut result = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if key == "$kah_source" || key == "encoding" {
                result.insert(key, map.next_value::<Value>()?);
                continue;
            }
            let child = match self.0 {
                Select::Fields(fields) => fields.get(&key),
                Select::Values(child) => Some(child.as_ref()),
                _ => None,
            };
            if let Some(child) = child {
                result.insert(key, map.next_value_seed(Seed(child))?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(Value::Object(result))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut array: A) -> std::result::Result<Value, A::Error> {
        let Select::Each(child) = self.0 else {
            while array.next_element::<IgnoredAny>()?.is_some() {}
            return Ok(Value::Null);
        };
        let mut result = vec![];
        while let Some(value) = array.next_element_seed(Seed(child))? {
            result.push(value);
        }
        Ok(Value::Array(result))
    }
    fn visit_str<E: serde::de::Error>(self, _: &str) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
}
pub(super) fn parse(reader: impl Read, selection: &Select) -> Result<Value> {
    let mut decoder = serde_json::Deserializer::from_reader(reader);
    let value = Seed(selection).deserialize(&mut decoder)?;
    decoder.end()?;
    Ok(value)
}
