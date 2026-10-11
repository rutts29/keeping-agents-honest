use super::*;
use crate::judge::{Judgment, Questions, jev};
use serde_json::Value;
impl Receipt {
    pub fn judgment(&self, elapsed_ms: u64) -> Result<Judgment> {
        let response: Value = serde_json::from_str(&self.response.body).or_else(|_| {
            if !(200..300).contains(&self.response.status) {
                Ok(Value::String(self.response.body.clone()))
            } else {
                Err(Error::new("JEV_RESPONSE_READ"))
            }
        })?;
        if !(200..300).contains(&self.response.status) {
            return Err(Error {
                code: if self.response.status == 413 || jev::context_rejection(&response) {
                    "JEV_CONTEXT_LIMIT"
                } else {
                    "JEV_HTTP_STATUS"
                }
                .into(),
                stage: "judge_http",
                status: Some(self.response.status),
                numeric_detail: None,
            });
        }
        let request: Value = serde_json::from_str(&self.request.body)
            .map_err(|_| Error::new("JEV_BATCH_REQUEST_JSON"))?;
        let questions: Questions = serde_json::from_value(request["questions"].clone())
            .map_err(|_| Error::new("JEV_BATCH_REQUEST_JSON"))?;
        jev::parse_response(&request["state"], &questions, &response, elapsed_ms)
    }
    pub(super) fn verdict(&self) -> Value {
        match self.judgment(0) {
            Ok(judgment) => serde_json::json!({"backend":judgment.backend,"model":judgment.model,
                "answers":judgment.answers,"probability_threshold":crate::judge::questions::COMPLETE_PROBABILITY,
                "confidence_threshold":crate::judge::questions::COMPLETE_CONFIDENCE}),
            Err(error) => serde_json::json!({"error":error.to_string(),"judged":false}),
        }
    }
}
