//! Codex prompt intake. The prompt hook only records the prompt digest and
//! starts a detached job. The job waits until native history holds that user
//! record, then runs the same intake as turn end. A turn-long first prompt then
//! has a run, staged rules and an intent wake before its first commit.
use kah_core::{observation::ObservationSnapshot, run_store::RunStore};
use kah_observer::{
    engine::{self, Outcome},
    session::{self, Binding},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

const REQUEST: &str = "prompt-intake.json";
/// Codex writes the user record after it builds the turn context (about 2.4 s
/// in the retained startup proof). Turn end still takes the intake if the
/// record arrives later than this.
pub const WAIT: Duration = Duration::from_secs(120);
/// Each read starts a native app-server; back off 0.5, 1, 2, then 4 s.
const INTERVAL: Duration = Duration::from_millis(500);
const MAX_INTERVAL: Duration = Duration::from_secs(4);

/// Hook side: one private file write and one detached spawn, no native read.
pub fn schedule(binding: &Binding, input: &Value) -> crate::Result<()> {
    if input["hook_event_name"] != "UserPromptSubmit" {
        return Ok(());
    }
    let Some(prompt) = input["prompt"].as_str().filter(|p| !p.trim().is_empty()) else {
        return Ok(());
    };
    let digest = digest(prompt);
    let turn = input["turn_id"]
        .as_str()
        .map_or_else(|| format!("prompt:{}", &digest[7..31]), str::to_owned);
    let directory = crate::observer_review::state::Store::directory(&binding.session)?;
    crate::unattended_report::save(
        &directory,
        REQUEST,
        &json!({"turn":turn,"prompt_sha256":digest}),
    )?;
    crate::observer_review::hook::spawn("__prompt-intake", &binding.session)
}

/// Detached job. Requests written while it runs are taken before it exits.
pub fn run(worker: &str) -> crate::Result<()> {
    let base = RunStore::default_state_root()?;
    let binding = session::Store::read(&base, worker)?
        .ok_or("PROMPT_INTAKE_SESSION stage=prompt_intake: session state absent")?
        .binding;
    let mut done = None;
    while let Some(request) = pending(worker)?.filter(|r| done.as_ref() != Some(r)) {
        let outcome = observe_when_recorded(&binding, &request, WAIT, INTERVAL, || {
            let mut client = engine::native_control(&binding)?;
            kah_codex::source::observe(&mut client, &binding.project, &binding.session)
                .map_err(|e| e.to_string())
        })?;
        match outcome {
            Some(Outcome { gap: Some(gap), .. }) => eprintln!("kah prompt intake gap: {gap}"),
            Some(_) => (),
            None => eprintln!(
                "PROMPT_INTAKE_PENDING stage=prompt_intake: no native record for {}; turn-end intake remains",
                request["turn"]
            ),
        }
        done = Some(request);
    }
    Ok(())
}

pub fn pending(worker: &str) -> crate::Result<Option<Value>> {
    crate::observer_review::state::read(
        &crate::observer_review::state::Store::directory(worker)?.join(REQUEST),
    )
}

/// Poll native history until it holds the prompt, then take the turn-end
/// intake. Delays double from `interval` up to 4 s. `None` means the session
/// is paused or the wait ended first.
pub fn observe_when_recorded(
    binding: &Binding,
    request: &Value,
    wait: Duration,
    mut interval: Duration,
    mut fetch: impl FnMut() -> Result<ObservationSnapshot, String>,
) -> crate::Result<Option<Outcome>> {
    let turn = request["turn"]
        .as_str()
        .ok_or("PROMPT_INTAKE_REQUEST stage=prompt_intake: turn absent")?;
    let wanted = request["prompt_sha256"]
        .as_str()
        .ok_or("PROMPT_INTAKE_REQUEST stage=prompt_intake: digest absent")?;
    let base = RunStore::default_state_root()?;
    let start = Instant::now();
    loop {
        if session::paused(&base, &binding.session) {
            return Ok(None);
        }
        match fetch() {
            Ok(snapshot) if recorded(&snapshot, wanted) => {
                let mut outcome = crate::observer_intake::snapshot(binding, turn, snapshot)?;
                crate::observer_review::dispatch::outcome(binding, &mut outcome)?;
                return Ok(Some(outcome));
            }
            // A brand-new thread can be unreadable until Codex writes it.
            Ok(_) | Err(_) if start.elapsed() < wait => {
                std::thread::sleep(interval.min(wait.saturating_sub(start.elapsed())));
                interval = (interval * 2).min(MAX_INTERVAL);
            }
            Ok(_) => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
}

/// The newest native user record is this prompt. An identical older prompt
/// can match early; that intake is then a no-op and turn end still runs.
fn recorded(snapshot: &ObservationSnapshot, wanted: &str) -> bool {
    [&snapshot.users, &snapshot.native_candidates]
        .into_iter()
        .flatten()
        .filter_map(|users| users.records().last())
        .any(|record| digest(record.content()) == wanted)
}

pub fn digest(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}
