//! #42 live fault: the Codex observer's gVisor exec-server dies under a
//! delivered review after a daemon restart. kah must replace the
//! observer process once per request on the SAME native session, redeliver
//! the SAME body. A second loss for that request retires it as not reviewed
//! (#52) and later triggers are still reviewed. The fixture only kills
//! containers and reads state: it never retries or kicks the queue itself.
use super::*;
use kah_cli::observer_review::{
    delivery_text,
    state::{Purpose, State, Store as Reviews},
};
use sha2::{Digest, Sha256};

const DOCKER: [&str; 2] = ["--host", "unix:///run/user/2001/docker.sock"];
const HOST_ERROR: &str = "Failed to create unified exec process:";
/// New user rules: each one makes kah queue its own intent-sync review
/// (observer_intake::after_sync). Fixture Drift wakes are retired unreviewed
/// (consume/retired.rs), so the fixture sends none.
const STEER_A: &str = "steer: Files named alpha.txt must always contain exactly ALPHA followed by one newline. Do not create or change any file now; reply NOTED without tools.";
const STEER_B: &str = "steer: Files named beta.txt must always contain exactly BETA followed by one newline. Do not create or change any file now; reply NOTED without tools.";
const STEER_C: &str = "steer: Files named gamma.txt must always contain exactly GAMMA followed by one newline. Do not create or change any file now; reply NOTED without tools.";

pub fn run(_root: &Path) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let config = home.join(".codex/config.toml");
    let before = fs::read(&config).unwrap();
    let (launch, mut codex) = crate::fixture::prepare(&home, "research", false);
    let mut command = launch.command(false);
    command
        .args(["--no-alt-screen", BLUE])
        .env_remove("CODEX_THREAD_ID");
    let mut panes = Panes(vec![Pane::start(&command).unwrap()]);
    let id = ready(&panes.0[0], "codex");
    codex.sessions.push(id.clone());
    let cleanup = super::review::Cleanup {
        home: home.clone(),
        worker: id.clone(),
    };
    let worker = panes.0[0].clone();
    let (courier, _courier) = courier(&home, &mut panes);
    let warm = warm_up(&id);
    let replaced = replaced_once(&id, &worker);
    let (retired, after) = second_loss(&id, &worker, &courier);
    let next = queue_continues(&id, &worker, &after);
    assert_eq!(fs::read(&config).unwrap(), before, "global config changed");
    let result = json!({"versions":versions(&home),"fixtureModels":{"codex":"gpt-6.1-sol/low"},"worker":id,"warmUp":warm,"replacedOnce":replaced,"secondLossRetired":retired,"queueContinues":next,"fixtureRetried":false,"passed":true});
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()).join("exec-loss-codex.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    drop(cleanup);
    drop(panes);
    result
}

/// The pane kah names in its human notice (`delivery_outcome::terminal`).
fn courier(home: &Path, panes: &mut Panes) -> (Pane, crate::fixture::Fixture) {
    let (launch, mut courier) = crate::fixture::prepare(home, "research", false);
    let mut command = launch.command(false);
    command.args(["--no-alt-screen", "You are a disposable notice courier. On kah observer notices reply NOTICE_RECEIVED. Do not use tools or answer questions."]).env_remove("CODEX_THREAD_ID");
    let receiver = Pane::start(&command).unwrap();
    courier.sessions.push(ready(&receiver, "codex"));
    let renamed = Command::new("herdr")
        .args(["agent", "rename", &receiver.id, "kah-reviewer"])
        .status()
        .unwrap();
    assert!(renamed.success());
    panes.0.push(receiver.clone());
    (receiver, courier)
}

/// Codex starts a stdio exec-server lazily, at the first command. The
/// worker's startup intent-sync review gives the observer a container.
fn warm_up(id: &str) -> Value {
    let state = super::review::quiescent(id, "codex");
    assert!(state.latest.is_some(), "no completed startup review");
    let containers = containers(&state.observer.workspace);
    assert!(
        !containers.is_empty(),
        "EXEC_LOSS_NO_EXECUTOR after startup"
    );
    json!({"latest":state.latest,"observerSession":state.observer.session,"observerPane":state.observer.pane,"containers":containers})
}

/// Fault 1 mirrors the pilot: the container dies while the observer is idle,
/// so the next delivered turn finds a dead transport.
fn replaced_once(id: &str, worker: &Pane) -> Value {
    let before = super::review::quiescent(id, "codex");
    let killed = kill(&before.observer.workspace);
    let request = steer(id, worker, STEER_A, &before);
    let mut watch = Watch::new(id, &request, &before);
    watch.until(|w| {
        assert!(
            w.terminal().is_none(),
            "first loss went terminal: {}",
            w.diagnostic()
        );
        w.state.pending.is_none()
    });
    let after = watch.state.clone();
    assert_eq!(after.latest.as_ref().unwrap()["request"], request.as_str());
    let saved = watch.directory.join(format!("results/{request}.json"));
    assert!(saved.is_file(), "no review result for {request}");
    let mut result = watch.replaced(&before);
    result["killedContainers"] = json!(killed);
    result["reviewResult"] = json!(true);
    result["latest"] = after.latest.unwrap();
    result
}

/// Fault 2 kills the replacement's container mid-turn, after its first
/// command completed, so the next command start meets a dead transport.
/// #52: after its one replacement, that second loss retires the request.
fn second_loss(id: &str, worker: &Pane, courier: &Pane) -> (Value, State) {
    let before = super::review::quiescent(id, "codex");
    let killed = kill(&before.observer.workspace);
    let request = steer(id, worker, STEER_B, &before);
    let mut watch = Watch::new(id, &request, &before);
    let mut second = None;
    watch.until(|w| {
        if second.is_none() && w.panes.len() == 2 && w.command_after_replacement() {
            second = Some(kill(&w.state.observer.workspace));
        }
        assert!(
            w.terminal().is_some() || w.pending(),
            "review completed: {}",
            w.diagnostic()
        );
        w.terminal().is_some()
    });
    let terminal = watch.terminal().unwrap();
    assert_eq!(terminal["exec_lost"], true, "{terminal}");
    assert_eq!(terminal["automatic_redelivery"], false, "{terminal}");
    assert_eq!(terminal["origin"], "delivery", "{terminal}");
    assert_eq!(terminal["retire"], true, "{terminal}");
    // The retry job retires it; the fixture does not.
    watch.until(|w| !w.pending());
    let retired = watch.retired(&terminal);
    let settled = watch.settle();
    let notice: Value = read(&watch.outcome.join("provider-block-notice.json")).unwrap();
    assert_eq!(notice["submitted"], true, "{notice}");
    let receiver = courier.logs("codex", None, None).unwrap();
    assert!(receiver.contains(&request), "courier lacks {request}");
    let mut result = watch.replaced(&before);
    result["killedContainers"] = json!([killed, second]);
    result["terminal"] = json!({"error":terminal["error"],"status":terminal["status"],"exec_lost":terminal["exec_lost"],"unanswered":terminal["unanswered"],"automatic_redelivery":terminal["automatic_redelivery"],"retire":terminal["retire"]});
    result["retired"] = retired;
    result["settled"] = settled;
    result["humanNotice"] = notice;
    (result, watch.state)
}

/// #52: the retired request no longer blocks the queue. kah admits the review
/// of a later user rule and finishes it; the fixture never kicks the queue.
fn queue_continues(id: &str, worker: &Pane, before: &State) -> Value {
    let request = steer(id, worker, STEER_C, before);
    let mut watch = Watch::new(id, &request, before);
    watch.until(|w| !w.pending());
    let path = watch.directory.join(format!("results/{request}.json"));
    let result = read(&path).unwrap_or_else(|| panic!("no result: {}", watch.diagnostic()));
    let replaced = watch
        .outcome
        .join(format!("exec-lost/{request}.json"))
        .is_file();
    json!({"request":request,"status":result["status"],"panes":watch.panes,"execLostRecord":replaced,"seconds":watch.started.elapsed().as_secs()})
}

/// Types a new user rule into the worker and returns the review request kah
/// makes from it. The dead transport cannot finish that review first.
fn steer(id: &str, worker: &Pane, text: &str, before: &State) -> String {
    let previous = before.latest.as_ref().map(|l| l["request"].clone());
    worker.prompt(text).unwrap();
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let state = Reviews::read(id).unwrap().unwrap();
        if let Some(pending) = &state.pending {
            assert_eq!(pending.purpose, Purpose::Steering, "{}", pending.id);
            return pending.id.clone();
        }
        let latest = state.latest.as_ref().map(|l| l["request"].clone());
        assert_eq!(
            latest, previous,
            "a review finished before it was seen pending"
        );
        assert!(
            Instant::now() < deadline,
            "EXEC_LOSS_NO_REQUEST after {text}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Running observer executors: the observer workspace is their project root.
fn containers(workspace: &Path) -> Vec<String> {
    let output = Command::new("/usr/bin/docker")
        .args(DOCKER)
        .args(["ps", "-q", "--filter"])
        .arg(format!("label=dev.kah.project={}", workspace.display()))
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .map(String::from)
        .collect()
}

fn kill(workspace: &Path) -> Vec<String> {
    let ids = containers(workspace);
    assert!(
        !ids.is_empty(),
        "EXEC_LOSS_NO_EXECUTOR: no executor labelled {}",
        workspace.display()
    );
    let output = Command::new("/usr/bin/docker")
        .args(DOCKER)
        .arg("kill")
        .args(&ids)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    ids
}

fn read(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Passive view of one faulted request: observer panes in order, gap codes and
/// delivered body digests kah recorded, and the observer's native rollout.
struct Watch {
    worker: String,
    request: String,
    directory: PathBuf,
    outcome: PathBuf,
    transcript: PathBuf,
    state: State,
    panes: Vec<String>,
    gaps: Vec<String>,
    digests: std::collections::BTreeSet<String>,
    rows_at_replacement: Option<usize>,
    started: Instant,
}

impl Watch {
    fn new(worker: &str, request: &str, before: &State) -> Self {
        let directory = Reviews::directory(worker).unwrap();
        Self {
            worker: worker.into(),
            request: request.into(),
            outcome: directory.join("delivery-outcome"),
            directory,
            transcript: before.observer.transcript.clone().unwrap(),
            state: before.clone(),
            panes: vec![before.observer.pane.clone().unwrap()],
            gaps: vec![],
            digests: Default::default(),
            rows_at_replacement: None,
            started: Instant::now(),
        }
    }

    fn until(&mut self, mut done: impl FnMut(&Self) -> bool) {
        let mut answered = std::collections::BTreeSet::new();
        loop {
            self.sample();
            let pane = self.state.observer.pane.clone().unwrap_or_default();
            // A resumed native pane can show the trust dialog again. That is
            // a native prompt, not a retry or a re-prompt by the fixture.
            if !pane.is_empty() && !answered.contains(&pane) {
                let screen = Pane { id: pane.clone() }.logs("codex", None, None);
                if screen.is_ok_and(|s| {
                    s.contains("Open restricted") || s.contains("Quick safety check:")
                }) {
                    crate::native_controller_support::native_consent(&pane, "codex");
                    answered.insert(pane);
                }
            }
            if done(self) {
                return;
            }
            assert!(
                self.started.elapsed() < Duration::from_secs(900),
                "EXEC_LOSS_TIMEOUT: {}",
                self.diagnostic()
            );
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    fn sample(&mut self) {
        self.state = Reviews::read(&self.worker).unwrap().unwrap();
        if let Some(pane) = &self.state.observer.pane
            && self.panes.last() != Some(pane)
        {
            self.panes.push(pane.clone());
            self.rows_at_replacement = Some(rows(&self.transcript).len());
        }
        if let Some(gap) = read(&self.directory.join("gap.json"))
            && gap["request"] == self.request.as_str()
            && let Some(code) = gap["error"]
                .as_str()
                .and_then(|e| e.split_whitespace().next())
            && self.gaps.last().map(String::as_str) != Some(code)
        {
            self.gaps.push(code.into());
        }
        if let Some(current) = read(&self.outcome.join("current.json"))
            && current["request"] == self.request.as_str()
        {
            self.digests
                .insert(current["body_digest"].as_str().unwrap().into());
        }
    }

    fn terminal(&self) -> Option<Value> {
        read(&self.outcome.join("terminal.json")).filter(|t| t["request"] == self.request.as_str())
    }

    /// The replacement process ran one command of the redelivered turn.
    fn command_after_replacement(&self) -> bool {
        let Some(from) = self.rows_at_replacement else {
            return false;
        };
        let rows = rows(&self.transcript);
        let Some(delivered) = rows[from..]
            .iter()
            .position(|r| user(r).is_some_and(|t| t.contains(&self.request)))
        else {
            return false;
        };
        rows[from + delivered..]
            .iter()
            .any(|r| command(r).is_some_and(|i| i["status"] == "completed"))
    }

    fn pending(&self) -> bool {
        self.state
            .pending
            .as_ref()
            .is_some_and(|p| p.id == self.request)
    }

    /// The durable result of a request retired as not reviewed (#52). Its
    /// reason follows the end of the turn: a decline or no reply.
    fn retired(&self, terminal: &Value) -> Value {
        let result: Value = read(
            &self
                .directory
                .join(format!("results/{}.json", self.request)),
        )
        .unwrap();
        assert_eq!(result["status"], "not_reviewed", "{result}");
        let code = if terminal["status"] == "delivery_refused" {
            "REVIEW_OBSERVER_REFUSED"
        } else {
            "REVIEW_OBSERVER_NO_REPORT"
        };
        assert_eq!(result["gap"]["code"], code, "{result}");
        json!({"status":result["status"],"gap":result["gap"],"pendingAfter":self.state.pending.as_ref().map(|p|&p.id)})
    }

    /// No further replacement, delivery or result change after retirement.
    fn settle(&mut self) -> Value {
        let panes = self.panes.clone();
        let name = format!("{}.json", self.request);
        let files = [
            self.outcome.join("exec-lost").join(&name),
            self.directory.join("results").join(&name),
        ];
        let records = files.clone().map(|f| fs::read(f).unwrap());
        let delivered = deliveries(&rows(&self.transcript), &self.request).len();
        let until = Instant::now() + Duration::from_secs(60);
        while Instant::now() < until {
            self.sample();
            std::thread::sleep(Duration::from_millis(250));
        }
        assert_eq!(self.panes, panes, "observer replaced after retirement");
        assert!(!self.pending(), "retired request pending again");
        assert!(!self.directory.join("observer-replace.json").exists());
        assert_eq!(files.map(|f| fs::read(f).unwrap()), records);
        assert_eq!(
            deliveries(&rows(&self.transcript), &self.request).len(),
            delivered,
            "retired request delivered again"
        );
        json!({"seconds":60,"paneUnchanged":true,"pendingCleared":true,"replaceMarkerAbsent":true,"execLostRecordUnchanged":true,"resultUnchanged":true,"nativeDeliveriesUnchanged":delivered})
    }

    /// Exactly one replacement on the same native session with the same body.
    fn replaced(&self, before: &State) -> Value {
        let record: Value = read(
            &self
                .outcome
                .join(format!("exec-lost/{}.json", self.request)),
        )
        .unwrap();
        assert_eq!(record["request"], self.request.as_str());
        assert_eq!(self.panes.len(), 2, "replacements: {:?}", self.panes);
        assert_eq!(self.state.observer.session, before.observer.session);
        // The same rollout may be named through the overlay's sessions
        // symlink or ~/.codex/sessions: compare files.
        let file = |p: &Option<PathBuf>| fs::canonicalize(p.as_ref().unwrap()).unwrap();
        assert_eq!(
            file(&self.state.observer.transcript),
            file(&before.observer.transcript)
        );
        assert_ne!(self.state.observer.pid, before.observer.pid);
        assert!(!self.directory.join("observer-replace.json").exists());
        // Use durable records. A sampler may miss a transient gap.json,
        // so sampled gaps and digests are evidence, not required observations.
        let current: Value = read(&self.outcome.join("current.json")).unwrap();
        assert_eq!(current["request"], self.request.as_str());
        let body = current["body_digest"].as_str().unwrap();
        assert!(self.digests.iter().all(|d| d == body), "{:?}", self.digests);
        let rows = rows(&self.transcript);
        let delivered = deliveries(&rows, &self.request);
        assert!(delivered.len() >= 2, "native deliveries: {delivered:?}");
        assert!(
            delivered.iter().all(|d| d == body),
            "{delivered:?} != {body}"
        );
        let failures = failures(&rows, &self.request);
        assert!(!failures.is_empty(), "no native exec-server loss row");
        let attempts = read(&self.directory.join("redelivery.json"))
            .map(|r| r["requests"][&self.request]["attempts"].clone());
        json!({"request":self.request,"observerSession":self.state.observer.session,"panes":self.panes,"observerPids":[before.observer.pid,self.state.observer.pid],"sampledGaps":self.gaps,"redeliveryAttempts":attempts,"execLostRecord":record,"bodyDigest":body,"nativeDeliveries":delivered.len(),"nativeExecFailures":failures,"seconds":self.started.elapsed().as_secs()})
    }

    fn diagnostic(&self) -> String {
        let rows = rows(&self.transcript);
        json!({"request":self.request,"panes":self.panes,"gaps":self.gaps,"digests":self.digests,"pending":self.state.pending.as_ref().map(|p|&p.id),"terminal":self.terminal(),"gap":read(&self.directory.join("gap.json")),"replaceMarker":read(&self.directory.join("observer-replace.json")),"nativeDeliveries":deliveries(&rows,&self.request).len(),"nativeExecFailures":failures(&rows,&self.request),"containers":containers(&self.state.observer.workspace),"seconds":self.started.elapsed().as_secs()}).to_string()
    }
}

fn rows(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Codex user message text, as the delivery-outcome reader reads it.
fn user(row: &Value) -> Option<String> {
    let payload = &row["payload"];
    (row["type"] == "response_item" && payload["type"] == "message" && payload["role"] == "user")
        .then(|| {
            payload["content"]
                .as_array()?
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
                .into()
        })
        .flatten()
}

fn command(row: &Value) -> Option<&Value> {
    let item = &row["payload"]["item"];
    (row["type"] == "event_msg"
        && row["payload"]["type"] == "item_completed"
        && item["type"] == "CommandExecution")
        .then_some(item)
}

/// Body digest of each native user message that carries `request`.
fn deliveries(rows: &[Value], request: &str) -> Vec<String> {
    rows.iter()
        .filter_map(user)
        .filter(|text| text.contains(request))
        .map(|text| {
            format!(
                "{:x}",
                Sha256::digest(delivery_text::body(&text).as_bytes())
            )
        })
        .collect()
}

/// Host-written exec-server loss rows after the first delivery of `request`.
fn failures(rows: &[Value], request: &str) -> Vec<Value> {
    let Some(first) = rows
        .iter()
        .position(|r| user(r).is_some_and(|t| t.contains(request)))
    else {
        return vec![];
    };
    rows[first..]
        .iter()
        .filter_map(|row| {
            let item = command(row)?;
            let text = item["aggregated_output"].as_str()?;
            (item["status"] == "failed" && text.starts_with(HOST_ERROR) && text.contains("exec-server transport disconnected"))
                .then(|| json!({"timestamp":row["timestamp"],"turn":row["payload"]["turn_id"],"output":text}))
        })
        .collect()
}
