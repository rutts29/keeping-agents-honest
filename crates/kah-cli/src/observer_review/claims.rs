//! Stop events provide native assistant text. Jev, not string matching, decides
//! whether it claims a phase/gate is complete. Work stays off the native hook.
use super::{hook, state::Store};
use crate::Result;
use kah_observer::{
    judge::{self, Answer, Judgment, Question},
    session::Binding,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub mod scope;
pub const VERSION: u32 = scope::VERSION;
pub fn questions() -> BTreeMap<String, Question> {
    scope::questions()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewClass {
    Commit,
    Gate,
}
pub fn review_class(judgment: &Judgment) -> Option<ReviewClass> {
    scope::review_class(judgment)
}
pub fn needs_review(judgment: &Judgment) -> bool {
    review_class(judgment).is_some()
}
/// Runs at every turn end, also without text: the job checks the
/// delivered-change review after the claim screen.
pub fn schedule(worker: &str, turn: &str, text: &str) -> Result<()> {
    let directory = Store::directory(worker)?.join("claim-events");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let id = format!("{:x}", Sha256::digest(serde_json::to_vec(&(turn, text))?));
    let path = directory.join(format!("{id}.json"));
    if !text.trim().is_empty() && !path.exists() && !directory.join(format!("{id}.done")).exists() {
        crate::unattended_report::save(
            &directory,
            &format!("{id}.json"),
            &json!({"turn":turn,"native_assistant_text":text}),
        )?;
    }
    hook::spawn("__checkpoint-claims", worker)
}
pub fn run(worker: &str) -> Result<()> {
    super::runtime::require_active(worker)?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("CHECKPOINT_HOME stage=trigger")?);
    let lease = super::job_lease::lease(worker, "__checkpoint-claims")?;
    run_with(worker, &lease, || {
        screen(worker, &home, |binding, input| {
            judge::routing::evaluate(
                &home,
                binding.judge,
                &binding.jev_model,
                input,
                &questions(),
            )
        })?;
        super::delivered::schedule(worker, &home)
    })
}
/// A Stop that writes its claim while a screen runs starts no job, so the
/// running job settles: an idle worker's last claim is never left unscreened.
/// A Stop with no text leaves only its spawn request, so that also settles.
pub fn run_with(worker: &str, lease: &fs::File, pass: impl FnMut() -> Result<()>) -> Result<()> {
    let directory = Store::directory(worker)?.join("claim-events");
    super::job_lease::until_requested_or(worker, "__checkpoint-claims", lease, pass, || {
        super::job_lease::has_json(&directory)
    })
}
/// `evaluate` is the Jev call; tests give a fake transport.
pub fn screen(
    worker: &str,
    home: &Path,
    evaluate: impl Fn(&Binding, &Value) -> std::result::Result<Judgment, judge::Error>,
) -> Result<()> {
    let directory = Store::directory(worker)?.join("claim-events");
    if !directory.exists() {
        return Ok(());
    }
    let binding = kah_observer::session::Store::read(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?
    .ok_or("CHECKPOINT_WORKER_STATE stage=trigger")?
    .binding;
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let mut input: Value =
            super::state::read(&path)?.ok_or("CHECKPOINT_CLAIM_MISSING stage=trigger")?;
        crate::privacy::scrub(&mut input);
        let record = match evaluate(&binding, &input) {
            Ok(judgment) => screened(worker, home, &input, judgment)?,
            Err(error) if error.code == "JEV_CONTEXT_LIMIT" => {
                oversized(worker, home, &input, &error)
            }
            Err(error) => return Err(error.into()),
        };
        crate::unattended_report::save(
            &directory,
            path.with_extension("done")
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            &record,
        )?;
        fs::remove_file(path)?;
    }
    Ok(())
}
fn screened(worker: &str, home: &Path, input: &Value, judgment: Judgment) -> Result<Value> {
    if let Some(class) = review_class(&judgment) {
        let uncertain = matches!(judgment.answers.get("phase_claim"),Some(Answer::Noul{noul}) if *noul<judge::questions::STEERING_PROBABILITY);
        enqueue(
            worker,
            home,
            class,
            json!({"source":input,"judgment":judgment,"question_set":VERSION,"uncertain":uncertain}),
        )?;
    }
    Ok(json!({"source":input,"judgment":judgment,"question_set":VERSION}))
}
/// Jev refused a reply too large for its context. The observer has no such
/// limit and reads in pages, so it judges the claim as a doubtful claim: the
/// same event an uncertain screen gives, with the whole reply. If that also
/// fails, the record keeps a visible gap. The claim file goes in both cases,
/// so one reply never blocks later Stops.
fn oversized(worker: &str, home: &Path, input: &Value, error: &judge::Error) -> Value {
    let refusal = json!({"error":error.to_string(),"body_digest":kah_observer::context_selection::digest(input),
        "route":"direct_observer_review","known_oversized":true});
    let event = json!({"source":input,"judgment":null,"jev_refusal":refusal,"question_set":VERSION,"uncertain":true});
    let mut record =
        json!({"source":input,"judgment":null,"jev_refusal":refusal,"question_set":VERSION});
    if let Err(failed) = enqueue(worker, home, ReviewClass::Commit, event) {
        eprintln!(
            "CHECKPOINT_CLAIM_UNJUDGED stage=claim_screen: {error}; observer hand-off: {failed}"
        );
        let mut gap = refusal;
        gap["handoff_error"] = failed.to_string().into();
        record["gap"] = gap;
    }
    record
}
/// Queue the claim for the observer review. `event` has the judged fields.
fn enqueue(worker: &str, home: &Path, class: ReviewClass, mut event: Value) -> Result<()> {
    let state = Store::read(worker)?
        .map(Ok)
        .unwrap_or_else(|| super::state::State::new(crate::relay_store::endpoint(worker)?, home))?;
    event["kind"] = "native_phase_claim".into();
    event["head"] = json!(super::tags::head(home, &state)?);
    event["review_class"] = match class {
        ReviewClass::Commit => "commit",
        ReviewClass::Gate => "gate",
    }
    .into();
    super::events::enqueue(worker, event)
}
