use super::http::HttpClient;
use crate::judge::{
    Error,
    batch::{Batch, Mode, RawResponse, Tokens, WireRequest},
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
impl HttpClient {
    pub(super) fn paced_scalar(&self, body: &str) -> Result<RawResponse, Error> {
        let Some(rate) = &self.rate_root else {
            return self.exchange_body(body, "scalar");
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::new("JEV_BATCH_CLOCK"))?
            .as_nanos();
        let id = format!(
            "scalar-{}-{now}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let calibration = Batch::calibration(rate)?;
        let request = WireRequest {
            id: id.clone(),
            body: body.into(),
            tokens: Tokens::estimate(body, calibration.as_ref())?,
        };
        let root = rate.join("scalar").join(&id);
        let mut outcome = Batch::new(&root, rate)?.run(&[request], self, Mode::Concurrent)?;
        Ok(outcome
            .responses
            .pop()
            .ok_or(Error::new("JEV_BATCH_BINDING"))?
            .response)
    }
}
