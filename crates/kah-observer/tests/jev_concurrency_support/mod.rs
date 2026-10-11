use kah_observer::{
    context_budget::Calibration,
    judge::batch::{Outcome, Tokens, WireRequest},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};
mod stub;
pub use stub::Stub;
pub struct Corpus {
    pub requests: Vec<WireRequest>,
    pub replies: BTreeMap<String, (u16, String, u64, u64)>,
    hashes: BTreeMap<PathBuf, String>,
}
impl Corpus {
    pub fn load() -> Self {
        let root = PathBuf::from(std::env::var_os("KAH_CONCURRENCY_CORPUS").unwrap());
        let mut records = fs::read_dir(root.join("responses"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("response-")
            })
            .collect::<Vec<_>>();
        records.sort_by_key(|p| {
            p.file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .trim_start_matches("response-")
                .parse::<usize>()
                .unwrap()
        });
        let calibration = calibration(&records);
        let mut corpus = Self {
            requests: vec![],
            replies: BTreeMap::new(),
            hashes: BTreeMap::new(),
        };
        for path in records {
            corpus.add_record(path, &calibration);
        }
        assert_eq!(corpus.requests.len(), 293);
        corpus
    }
    fn add_record(&mut self, path: PathBuf, calibration: &Calibration) {
        let data = fs::read(&path).unwrap();
        self.hashes.insert(path, digest(&data));
        let value: Value = serde_json::from_slice(&data).unwrap();
        let body_path = PathBuf::from(value["job"]["path"].as_str().unwrap());
        let body = fs::read_to_string(&body_path).unwrap();
        assert_eq!(
            value["job"]["digest"],
            format!("sha256:{}", digest(body.as_bytes()))
        );
        self.hashes.insert(body_path, digest(body.as_bytes()));
        let id = format!("phase1-{}", value["index"].as_u64().unwrap());
        let exact = value["response"]["usage"]["input_tokens"].as_u64();
        let mut tokens = Tokens::estimate(&body, Some(calibration)).unwrap();
        tokens.recorded_exact = exact;
        let status = if value["response"].is_object() {
            200
        } else {
            value["error_response"]["status"].as_u64().unwrap() as u16
        };
        let response = if status == 200 {
            &value["response"]
        } else {
            &value["error_response"]["body"]
        };
        self.replies.insert(
            id.clone(),
            (
                status,
                serde_json::to_string(response).unwrap(),
                exact.unwrap_or(0),
                value["elapsed_ms"].as_u64().unwrap(),
            ),
        );
        self.requests.push(WireRequest { id, body, tokens });
    }
    pub fn assert_unchanged(&self) {
        for (path, expected) in &self.hashes {
            assert_eq!(&digest(&fs::read(path).unwrap()), expected);
        }
    }
    pub fn assert_answers(&self, result: &Outcome) {
        assert_eq!(result.responses.len(), self.requests.len());
        for (request, response) in self.requests.iter().zip(&result.responses) {
            let (status, body, _, _) = &self.replies[&request.id];
            assert_eq!(&response.response.status, status);
            assert_eq!(&response.response.body, body);
            assert_eq!(response.request.body, request.body);
        }
    }
}
fn calibration(records: &[PathBuf]) -> Calibration {
    let mut bytes = 0;
    let mut tokens = 0;
    for path in records {
        let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        if let Some(exact) = value["response"]["usage"]["input_tokens"].as_u64() {
            let body: Value =
                serde_json::from_slice(&fs::read(value["job"]["path"].as_str().unwrap()).unwrap())
                    .unwrap();
            bytes += body["state"].to_string().len() + body["questions"].to_string().len();
            tokens += exact;
        }
    }
    Calibration {
        input_utf8_bytes: bytes as u64,
        input_tokens: tokens,
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
