//! Empirical planning/reporting only. Never rejects, cuts or caps a request.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Calibration {
    pub samples: usize,
    pub input_tokens: u64,
    pub input_utf8_bytes: u64,
    pub minimum_ratio: f64,
    pub maximum_ratio: f64,
    pub source_digests: BTreeMap<String, String>,
}
impl Calibration {
    pub fn from_archive(root: &Path) -> Result<Self> {
        let mut ratios = vec![];
        let mut input_tokens = 0;
        let mut input_utf8_bytes = 0;
        let mut source_digests = BTreeMap::new();
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let raw = fs::read(&path)?;
            let value: Value = serde_json::from_slice(&raw)?;
            let metrics = &value["judgment"]["metrics"];
            let Some(bytes) = metrics["input_utf8_bytes"].as_u64().filter(|b| *b > 0) else {
                continue;
            };
            let tokens = metrics["input_tokens"]
                .as_u64()
                .ok_or("MEASURE_CALIBRATION stage=estimate")?;
            ratios.push(tokens as f64 / bytes as f64);
            input_tokens += tokens;
            input_utf8_bytes += bytes;
            source_digests.insert(path.to_string_lossy().into(), commit_check::digest(&raw));
        }
        if ratios.is_empty() {
            return Err("MEASURE_CALIBRATION_MISSING stage=estimate".into());
        }
        Ok(Self {
            samples: ratios.len(),
            input_tokens,
            input_utf8_bytes,
            minimum_ratio: ratios.iter().copied().fold(f64::INFINITY, f64::min),
            maximum_ratio: ratios.iter().copied().fold(0.0, f64::max),
            source_digests,
        })
    }
    pub fn estimate(&self, bytes: u64) -> Value {
        let tokens = bytes as f64 * self.input_tokens as f64 / self.input_utf8_bytes as f64;
        json!({"empirical_input_tokens":tokens,"observed_ratio_range_tokens":[bytes as f64*self.minimum_ratio,bytes as f64*self.maximum_ratio],
            "estimated_usd":tokens*0.042/1_000_000.0,"model":"jev-1.13.0",
            "price_source":"https://docs.typesafe.ai/models checked live2026-10-10: $0.042/M input; no output charge",
            "actual_tokens":null,"planning_only":true,"not_a_tokenizer_or_bound":true,"calibration_samples":self.samples})
    }
}
