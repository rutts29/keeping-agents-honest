//! Walk reference envelopes without materializing source bodies.
use crate::Result;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{fmt, io::Read};
#[derive(Clone)]
pub(super) struct Reference {
    pub digest: String,
    pub json: bool,
}
struct Scan<'a> {
    refs: &'a mut Vec<Reference>,
    capture: bool,
}
impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = Option<String>;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        d: D,
    ) -> std::result::Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Scan<'_> {
    type Value = Option<String>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("lossless reference graph")
    }
    fn visit_map<M: MapAccess<'de>>(
        self,
        mut map: M,
    ) -> std::result::Result<Self::Value, M::Error> {
        let mut digest = None;
        let mut encoding = None;
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value_seed(Scan {
                refs: self.refs,
                capture: matches!(key.as_str(), "$kah_source" | "encoding"),
            })?;
            match key.as_str() {
                "$kah_source" => digest = value,
                "encoding" => encoding = value,
                _ => {}
            }
        }
        if let Some(digest) = digest {
            if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(serde::de::Error::custom(
                    "REVIEW_SOURCE_ID stage=source_view",
                ));
            }
            let json = match encoding.as_deref() {
                Some("json") => true,
                Some("utf8") => false,
                _ => {
                    return Err(serde::de::Error::custom(
                        "REVIEW_SOURCE_ENCODING stage=source_view",
                    ));
                }
            };
            self.refs.push(Reference { digest, json });
        }
        Ok(None)
    }
    fn visit_seq<A: SeqAccess<'de>>(
        self,
        mut array: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        while array
            .next_element_seed(Scan {
                refs: self.refs,
                capture: false,
            })?
            .is_some()
        {}
        Ok(None)
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
        Ok(self.capture.then(|| v.to_owned()))
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
}
pub(super) fn collect(reader: impl Read) -> Result<Vec<Reference>> {
    let mut refs = vec![];
    let mut decoder = serde_json::Deserializer::from_reader(reader);
    Scan {
        refs: &mut refs,
        capture: false,
    }
    .deserialize(&mut decoder)?;
    decoder.end()?;
    Ok(refs)
}
