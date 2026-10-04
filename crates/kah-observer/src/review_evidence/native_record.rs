//! Host-derived native record context. Encoded as one string so the existing CAS
//! stores the complete annotation source once, alongside the complete transcript.
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeRecord {
    pub start_byte: usize,
    pub end_byte: usize,
    pub context: Value,
}
pub mod encoded {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        records: &[NativeRecord],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer
            .serialize_str(&serde_json::to_string(records).map_err(serde::ser::Error::custom)?)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<NativeRecord>, D::Error> {
        let text = String::deserialize(deserializer)?;
        serde_json::from_str(&text).map_err(serde::de::Error::custom)
    }
}
