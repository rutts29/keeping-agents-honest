use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::judge::batch) struct Start {
    pub ticket: String,
    pub request: String,
    pub at_ms: u64,
    pub estimated: f64,
    pub charged: f64,
    pub input_utf8_bytes: u64,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Flight {
    pub pid: u32,
    pub stamp: Option<String>,
    pub review: PathBuf,
}
#[derive(Default, Serialize, Deserialize)]
pub(super) struct Window {
    pub ceiling: Limits,
    pub effective: Limits,
    pub starts: Vec<Start>,
    pub active: BTreeMap<String, Flight>,
    sequence: u64,
}
impl Window {
    pub fn prune(&mut self, now: u64) {
        self.starts.retain(|s| now.saturating_sub(s.at_ms) < 1000);
        self.active
            .retain(|_, f| f.stamp.is_some() && crate::session::process_stamp(f.pid) == f.stamp);
    }
    pub fn wait(&self, now: u64, tokens: f64) -> Duration {
        // Unknown/over-window estimates never strand an intact request.
        if self.starts.is_empty() {
            return Duration::ZERO;
        }
        if self.starts.len() as f64 + 1.0 <= self.effective.requests_per_second
            && self.starts.iter().map(|s| s.charged).sum::<f64>() + tokens
                <= self.effective.tokens_per_second
        {
            return Duration::ZERO;
        }
        Duration::from_millis(1000_u64.saturating_sub(now.saturating_sub(self.starts[0].at_ms)))
    }
    pub fn admit(&mut self, now: u64, request: &str, root: &Path, tokens: &Tokens) -> Start {
        self.sequence += 1;
        let ticket = self.sequence.to_string();
        let start = Start {
            ticket: ticket.clone(),
            request: request.into(),
            at_ms: now,
            estimated: tokens.estimated,
            charged: tokens.charge(),
            input_utf8_bytes: tokens.input_utf8_bytes,
        };
        self.starts.push(start.clone());
        let pid = std::process::id();
        self.active.insert(
            ticket,
            Flight {
                pid,
                stamp: crate::session::process_stamp(pid),
                review: root.into(),
            },
        );
        start
    }
    pub fn feedback(&mut self, response: &RawResponse) {
        self.headers(&response.headers);
        if matches!(response.status, 429 | 529) {
            // Multiplicative decrease; recovery adds one documented request share.
            self.effective.requests_per_second =
                (self.effective.requests_per_second / 2.0).max(1.0);
            self.effective.tokens_per_second /= 2.0;
        } else if response.status < 400 {
            self.effective.requests_per_second =
                (self.effective.requests_per_second + 1.0).min(self.ceiling.requests_per_second);
            self.effective.tokens_per_second = (self.effective.tokens_per_second
                + self.ceiling.tokens_per_second / self.ceiling.requests_per_second)
                .min(self.ceiling.tokens_per_second);
        }
    }
    fn headers(&mut self, headers: &BTreeMap<String, String>) {
        let seconds = headers
            .get("x-ratelimit-window-seconds")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        if seconds <= 0.0 || !seconds.is_finite() {
            return;
        }
        if let Some(n) = positive(headers, "x-ratelimit-limit-requests") {
            self.ceiling.requests_per_second = n / seconds;
            self.effective.requests_per_second =
                self.effective.requests_per_second.min(n / seconds);
        }
        if let Some(n) = positive(headers, "x-ratelimit-limit-tokens") {
            self.ceiling.tokens_per_second = n / seconds;
            self.effective.tokens_per_second = self.effective.tokens_per_second.min(n / seconds);
        }
    }
}
fn positive(headers: &BTreeMap<String, String>, name: &str) -> Option<f64> {
    headers
        .get(name)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
}
