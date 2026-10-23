use super::*;

pub(super) struct AppServer {
    pub(super) child: Child,
    pub(super) stdin: ChildStdin,
    pub(super) stdout: BufReader<ChildStdout>,
    pub(super) next_request_id: u64,
}

/// The `initialize` request names this product to the Codex App Server.
pub(super) fn initialize_params() -> Value {
    json!({
        "clientInfo": {
            "name": "kah",
            "title": null,
            "version": env!("CARGO_PKG_VERSION"),
        },
        "capabilities": {
            "experimentalApi": false,
            "requestAttestation": false,
        },
    })
}

impl AppServer {
    pub(super) fn start() -> Result<Self, SourceError> {
        let program = env::var_os("KAH_CODEX_BIN").unwrap_or_else(|| "codex".into());
        let mut child = Command::new(program)
            .args(["app-server", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                SourceError::new(
                    "CODEX_COULD_NOT_LAUNCH_CODEX_APP_SERVER",
                    "source_server",
                    format!("could not launch Codex App Server: {error}"),
                )
            })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            SourceError::new(
                "CODEX_COULD_NOT_OPEN_CODEX_APP_SERVER_STDIN",
                "source_server",
                "could not open Codex App Server stdin",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            SourceError::new(
                "CODEX_COULD_NOT_OPEN_CODEX_APP_SERVER_STDOUT",
                "source_server",
                "could not open Codex App Server stdout",
            )
        })?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_request_id: 1,
        })
    }

    pub(super) fn initialize(&mut self) -> Result<(), SourceError> {
        self.request("initialize", initialize_params())?;
        self.notify("initialized", json!({}))
    }

    pub(super) fn latest_thread_id(&mut self, canonical_root: &str) -> Result<String, SourceError> {
        let mut cursor = None;
        let mut seen_cursors = BTreeSet::new();
        let mut best: Option<ThreadCandidate> = None;
        let mut tied_best = false;
        let mut exhausted = false;

        for _ in 0..MAX_THREAD_LIST_PAGES {
            let response = self.request(
                "thread/list",
                json!({
                    "cursor": cursor,
                    "limit": 100,
                    "sortKey": "recency_at",
                    "sortDirection": "desc",
                    "sourceKinds": THREAD_SOURCE_KINDS,
                    "cwd": canonical_root,
                    "useStateDbOnly": true,
                }),
            )?;
            let (candidates, next_cursor) = parse_thread_page(response, canonical_root)?;
            update_latest(&mut best, &mut tied_best, candidates);
            let Some(next_cursor) = next_cursor else {
                exhausted = true;
                break;
            };
            if !seen_cursors.insert(next_cursor.clone()) {
                return Err(SourceError::new(
                    "CODEX_APP_SERVER_RETURNED_A_REPEATED_PAGINATION_CURSOR",
                    "source_server",
                    "Codex App Server returned a repeated pagination cursor",
                ));
            }
            cursor = Some(next_cursor);
        }

        if !exhausted {
            return Err(SourceError::new(
                "CODEX_APP_SERVER_THREAD_LISTING_EXCEEDS_THE_PAGE_LIMIT",
                "source_server",
                "Codex App Server thread listing exceeds the page limit",
            ));
        }
        if tied_best {
            return Err(SourceError::new(
                "CODEX_LATEST_CODEX_SESSION_IS_AMBIGUOUS_FOR_THE_CURRENT_REPOSITORY",
                "source_server",
                "latest Codex session is ambiguous for the current repository",
            ));
        }
        best.map(|candidate| candidate.id).ok_or_else(|| {
            SourceError::new(
                "CODEX_NO_CODEX_SESSION_MATCHES_THE_CURRENT_REPOSITORY",
                "source_server",
                "no Codex session matches the current repository",
            )
        })
    }

    pub(super) fn read_thread(&mut self, thread_id: &str) -> Result<Value, SourceError> {
        self.request(
            "thread/read",
            json!({
                "threadId": thread_id,
                "includeTurns": true,
            }),
        )
    }

    pub(super) fn observe_goal(&mut self, thread_id: &str) -> Result<NativeGoal, SourceError> {
        let response = match self.request(
            "thread/goal/get",
            json!({
                "threadId": thread_id,
            }),
        ) {
            Ok(response) => response,
            Err(error)
                if error.code == "CODEX_RPC_REJECTED"
                    && error.message == "Codex App Server rejected thread/goal/get" =>
            {
                return Ok(NativeGoal::Unknown);
            }
            Err(error) => return Err(error),
        };
        parse_native_goal(response, thread_id)
    }

    pub(super) fn request(&mut self, method: &str, params: Value) -> Result<Value, SourceError> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.write_json(&json!({
            "id": id,
            "method": method,
            "params": params,
        }))?;

        loop {
            let response = self.read_json_line()?;
            let object = response.as_object().ok_or_else(|| {
                SourceError::new(
                    "CODEX_APP_SERVER_EMITTED_A_NON_OBJECT_PROTOCOL_MESSAGE",
                    "source_server",
                    "Codex App Server emitted a non-object protocol message",
                )
            })?;
            // A message with a method comes from the server: a notification is
            // skipped, a request is declined under its own id.
            if object.get("method").is_some() {
                if let Some(server_id) = object.get("id") {
                    self.write_json(&json!({
                        "id": server_id,
                        "error": {"code": -32601, "message": "this client handles no server requests"},
                    }))?;
                }
                continue;
            }
            let response_id = object.get("id").and_then(Value::as_u64).ok_or_else(|| {
                SourceError::new(
                    "CODEX_APP_SERVER_RESPONSE_IS_MISSING_A_NUMERIC_ID",
                    "source_server",
                    "Codex App Server response is missing a numeric ID",
                )
            })?;
            if response_id != id {
                return Err(SourceError::new(
                    "CODEX_APP_SERVER_RETURNED_AN_UNEXPECTED_RESPONSE_ID",
                    "source_server",
                    "Codex App Server returned an unexpected response ID",
                ));
            }
            if object.get("error").is_some() {
                return Err(SourceError::new(
                    "CODEX_RPC_REJECTED",
                    "source_server",
                    format!("Codex App Server rejected {method}"),
                ));
            }
            return object.get("result").cloned().ok_or_else(|| {
                SourceError::new(
                    "CODEX_APP_SERVER_RESPONSE_HAS_NO_RESULT",
                    "source_server",
                    format!("Codex App Server {method} response has no result"),
                )
            });
        }
    }

    pub(super) fn notify(&mut self, method: &str, params: Value) -> Result<(), SourceError> {
        self.write_json(&json!({
            "method": method,
            "params": params,
        }))
    }

    pub(super) fn write_json(&mut self, value: &Value) -> Result<(), SourceError> {
        let message = serde_json::to_vec(value).map_err(|error| {
            SourceError::new(
                "CODEX_COULD_NOT_ENCODE_CODEX_APP_SERVER_REQUEST",
                "source_server",
                format!("could not encode Codex App Server request: {error}"),
            )
        })?;
        self.stdin
            .write_all(&message)
            .and_then(|()| self.stdin.write_all(b"\n"))
            .and_then(|()| self.stdin.flush())
            .map_err(|error| {
                SourceError::new(
                    "CODEX_COULD_NOT_WRITE_TO_CODEX_APP_SERVER",
                    "source_server",
                    format!("could not write to Codex App Server: {error}"),
                )
            })
    }

    pub(super) fn read_json_line(&mut self) -> Result<Value, SourceError> {
        let mut line = Vec::new();
        let read = (&mut self.stdout)
            .take((MAX_PROTOCOL_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|error| {
                SourceError::new(
                    "CODEX_COULD_NOT_READ_CODEX_APP_SERVER_RESPONSE",
                    "source_server",
                    format!("could not read Codex App Server response: {error}"),
                )
            })?;
        if read == 0 {
            return Err(SourceError::new(
                "CODEX_APP_SERVER_CLOSED_BEFORE_A_RESPONSE",
                "source_server",
                "Codex App Server closed before a response",
            ));
        }
        if line.last() != Some(&b'\n') {
            return Err(SourceError::new(
                if line.len() > MAX_PROTOCOL_LINE_BYTES {
                    "CODEX_PROTOCOL_LIMIT"
                } else {
                    "CODEX_PROTOCOL_TRUNCATED"
                },
                "source_server",
                if line.len() > MAX_PROTOCOL_LINE_BYTES {
                    "Codex App Server response exceeds the 128 MiB size limit"
                } else {
                    "Codex App Server response is truncated"
                },
            ));
        }
        line.pop();
        serde_json::from_slice(&line).map_err(|_| {
            SourceError::new(
                "CODEX_APP_SERVER_EMITTED_MALFORMED_JSON",
                "source_server",
                "Codex App Server emitted malformed JSON",
            )
        })
    }

    pub(super) fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ThreadCandidate {
    pub(super) id: String,
    pub(super) recency: i64,
}

fn update_latest(
    best: &mut Option<ThreadCandidate>,
    tied_best: &mut bool,
    candidates: Vec<ThreadCandidate>,
) {
    for candidate in candidates {
        match &*best {
            None => *best = Some(candidate),
            Some(current) if candidate.recency > current.recency => {
                *best = Some(candidate);
                *tied_best = false;
            }
            Some(current) if candidate.recency == current.recency && candidate.id != current.id => {
                *tied_best = true;
            }
            _ => {}
        }
    }
}
