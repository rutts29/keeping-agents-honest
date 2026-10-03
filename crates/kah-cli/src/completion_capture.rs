//! The private archive stores exactly the scrubbed body supplied to Jev.
use crate::privacy;
use kah_observer::judge::{Error, Judgment, Questions};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Request {
    model: String,
    state: Value,
    questions: Questions,
    redactions: usize,
}
impl Request {
    pub fn new(model: &str, state: &Value, questions: &Questions) -> Result<Self, String> {
        kah_observer::judge::validate_request(state, questions).map_err(|e| e.to_string())?;
        if model.is_empty() || model.len() > 100 {
            return Err("COMPLETION_REQUEST_MODEL stage=completion_capture".into());
        }
        let mut body = json!({"model":model,"state":state,"questions":questions});
        let redactions = privacy::scrub(&mut body);
        Ok(Self {
            model: body["model"].as_str().unwrap().into(),
            state: body["state"].take(),
            questions: serde_json::from_value(body["questions"].take())
                .map_err(|_| "COMPLETION_REQUEST_SHAPE stage=completion_capture")?,
            redactions,
        })
    }
    pub fn state(&self) -> &Value {
        &self.state
    }
    pub fn questions(&self) -> &Questions {
        &self.questions
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn body(&self) -> Value {
        json!({"model":self.model,"state":self.state,"questions":self.questions})
    }
}

pub fn save(
    home: &Path,
    session: &str,
    commit: &str,
    channel: &str,
    request: &Request,
    result: &Result<Judgment, Error>,
) -> Result<PathBuf, String> {
    save_versioned(
        home,
        session,
        commit,
        channel,
        request,
        result,
        kah_observer::judge::questions::VERSION,
    )
}
pub fn save_versioned(
    home: &Path,
    session: &str,
    commit: &str,
    channel: &str,
    request: &Request,
    result: &Result<Judgment, Error>,
    question_set: u32,
) -> Result<PathBuf, String> {
    let base = home.join(".local/share/kah-eval");
    kah_core::run_store::ensure_private_directory(&base)
        .map_err(|e| format!("COMPLETION_CAPTURE_DIRECTORY stage=completion_capture: {e}"))?;
    let directory = base.join("completion");
    kah_core::run_store::ensure_private_directory(&directory)
        .map_err(|e| format!("COMPLETION_CAPTURE_DIRECTORY stage=completion_capture: {e}"))?;
    let body = request.body();
    let digest = format!("{:x}", Sha256::digest(body.to_string().as_bytes()));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "COMPLETION_CAPTURE_CLOCK stage=completion_capture")?
        .as_nanos();
    let name = format!("{nonce}-{}-{}.json", std::process::id(), &digest[..16]);
    let value = json!({"schema":1,"session":session,"commit":commit,"channel":channel,"request":body,"request_digest":digest,
        "redactions":request.redactions,"scrubbed_before_evaluation":true,"question_set":question_set,
        "thresholds":{"probability":kah_observer::judge::questions::COMPLETE_PROBABILITY,"confidence":kah_observer::judge::questions::COMPLETE_CONFIDENCE},
        "judgment":result.as_ref().ok(),"error":result.as_ref().err().map(ToString::to_string)});
    crate::unattended_report::save(&directory, &name, &value)
        .map_err(|e| format!("COMPLETION_CAPTURE_IO stage=completion_capture: {e}"))?;
    Ok(directory.join(name))
}
