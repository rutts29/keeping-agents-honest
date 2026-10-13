//! Durable SHA-bound jobs; no native observer and no store lock across Jev.
use super::super::{
    option1,
    state::{self, State, Store},
};
use super::{Report, gap, host, input, retry};
use crate::Result;
use kah_observer::{
    commit_check::Input,
    commit_review::DoubtLine,
    judge::{
        availability,
        batch::{Batch, WireRequest},
        jev::HttpClient,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct Job {
    head: String,
    input: Option<Input>,
    requests: Vec<WireRequest>,
    key_map: Value,
    policy: DoubtLine,
    gap: Option<String>,
}
pub fn run(worker: &str) -> Result<()> {
    let binding = host::binding(worker)?;
    if !host::scope(&binding)? {
        return Err("UNTRUSTED_SCOPE stage=commit".into());
    }
    let root = host::root(worker)?;
    let _lease = Store::delivery_at(&root.join("worker"), false)?;
    let home = PathBuf::from(env::var_os("HOME").ok_or("UNTRUSTED_HOME stage=commit")?);
    let mut state = State::new(crate::relay_store::endpoint(worker)?, &home)?;
    // Reuse the installed read-only helper, not an observer workspace/session.
    state.observer.workspace = home.join(".local");
    loop {
        crate::helper_upgrade::between_samples(Some(_lease.lease()))?;
        if kah_observer::session::paused(
            &kah_core::run_store::RunStore::default_state_root()?,
            worker,
        ) {
            return Ok(());
        }
        if let Some(job) = state::read::<Job>(&root.join("job.json"))? {
            finish(worker, &root, &home, &job, _lease.lease())?;
            fs::remove_file(root.join("job.json"))?;
        }
        let paths = host::entries(&root.join("events"))?;
        let Some(last) = paths.last() else {
            return Ok(());
        };
        let event: Value = state::read(last)?.ok_or("UNTRUSTED_EVENT_MISSING stage=commit")?;
        let head = if binding.project.join(".git").exists() {
            super::super::tags::head(&home, &state)?
        } else {
            None
        };
        if let Some(head) = head {
            let previous: Option<Value> = state::read(&root.join("baseline.json"))?;
            if previous.as_ref().is_none_or(|v| v["head"] != head) {
                let job = retry::run(worker, &root, &head, _lease.lease(), || {
                    prepare(
                        worker,
                        &home,
                        &state,
                        previous.as_ref().and_then(|v| v["head"].as_str()),
                        &head,
                        &event,
                    )
                })?;
                crate::unattended_report::save(&root, "job.json", &json!(job))?;
                continue;
            }
        }
        for path in paths {
            fs::remove_file(path)?;
        }
    }
}
fn prepare(
    worker: &str,
    home: &Path,
    state: &State,
    base: Option<&str>,
    head: &str,
    event: &Value,
) -> Result<Job> {
    let policy = option1::journal::policy(home)?;
    let result = prepared(worker, home, state, base, head, event);
    Ok(match result {
        Ok((input, requests, key_map)) => {
            let gap = terminal(worker, &requests)?;
            Job {
                head: head.into(),
                input: Some(input),
                requests,
                key_map,
                policy,
                gap,
            }
        }
        Err(error) if availability::outage(&error.to_string()) => return Err(error),
        Err(error) => Job {
            head: head.into(),
            input: None,
            requests: vec![],
            key_map: json!({}),
            policy,
            gap: Some(error.to_string()),
        },
    })
}
fn prepared(
    worker: &str,
    home: &Path,
    state: &State,
    base: Option<&str>,
    head: &str,
    event: &Value,
) -> Result<(Input, Vec<WireRequest>, Value)> {
    let binding = host::binding(worker)?;
    if state.worker.provider == "claude" {
        let snapshot = crate::claude_hook::snapshot(event, &binding)?;
        let outcome =
            kah_observer::engine::synchronize_snapshot(&binding, "untrusted-commit", snapshot)?;
        if let Some(gap) = outcome.gap {
            return Err(gap.into());
        }
    } else {
        let outcome = kah_observer::engine::synchronize(&binding, "untrusted-commit")?;
        if let Some(gap) = outcome.gap {
            return Err(gap.into());
        }
    }
    let history = option1::authority::history(worker)?;
    let standing = kah_observer::current_rules::standing_sources(&binding.project)?;
    let context = option1::capture::project(home, state, base, head)?;
    let input = input::build(&history, &standing, &context)?;
    let rate = kah_core::run_store::RunStore::default_state_root()?.join("jev-rate");
    let calibration = Batch::calibration(&rate)?;
    let (requests, map) = option1::requests::build(&input, calibration.as_ref())?;
    Ok((input, requests, map))
}
fn finish(worker: &str, root: &Path, home: &Path, job: &Job, lease: &fs::File) -> Result<()> {
    let rate = kah_core::run_store::RunStore::default_state_root()?.join("jev-rate");
    let batch = Batch::new(&root.join("literal").join(&job.head), &rate)?;
    let report = retry::run(worker, root, &job.head, lease, || {
        if let Some(reason) = &job.gap {
            gap(worker, &job.head, &job.policy, reason)
        } else {
            super::evaluate(
                worker,
                &job.head,
                &job.policy,
                &batch,
                &job.requests,
                job.key_map.clone(),
                &HttpClient::from_home(home)?,
            )
        }
    })?;
    save(root, job, &report)
}
fn terminal(worker: &str, requests: &[WireRequest]) -> Result<Option<String>> {
    let rate = kah_core::run_store::RunStore::default_state_root()?.join("jev-rate");
    if super::oversized(requests, Batch::calibration(&rate)?.as_ref())? {
        return Ok(Some("too_large".into()));
    }
    let summary = host::summary(worker)?;
    for request in requests {
        let body: Value = serde_json::from_str(&request.body)?;
        let digest = kah_observer::context_selection::digest(&body);
        if summary["reports"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|r| {
                r["record"]["gaps"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|g| g["known_oversized"] == true && g["body_digest"] == digest)
            })
        {
            return Ok(Some("too_large".into()));
        }
    }
    Ok(None)
}
fn save(root: &Path, job: &Job, report: &Report) -> Result<()> {
    let results = root.join("results");
    kah_core::run_store::ensure_private_directory(&results)?;
    let sources = root.join("sources");
    kah_core::run_store::ensure_private_directory(&sources)?;
    crate::unattended_report::save(&sources, &format!("{}.json", job.head), &json!(job))?;
    crate::unattended_report::save(&results, &format!("{}.json", job.head), &json!(report))?;
    if report.flagged || !report.record.gaps.is_empty() {
        let notices = root.join("notices");
        kah_core::run_store::ensure_private_directory(&notices)?;
        crate::unattended_report::save(&notices, &format!("{}.json", job.head), &json!(report))?;
    }
    crate::unattended_report::save(
        root,
        "baseline.json",
        &json!({"head":job.head,"status":report.status}),
    )?;
    if root.join("pending.json").exists() {
        fs::remove_file(root.join("pending.json"))?;
    }
    Ok(())
}
