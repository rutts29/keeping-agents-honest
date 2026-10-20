//! Event-driven Git metadata sampling; never a host Git call in Research.
use super::{
    hook,
    state::{State, Store},
};
use crate::Result;
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn schedule(worker: &str) -> Result<()> {
    let root = Store::directory(worker)?.join("tag-events");
    kah_core::run_store::ensure_private_directory(&root)?;
    crate::unattended_report::save(
        &root,
        &format!("{}.json", crate::relay_store::nonce()),
        &json!({"at_ms":kah_observer::session::now_ms()}),
    )?;
    hook::spawn("__checkpoint-tags", worker)
}
pub fn run(worker: &str) -> Result<()> {
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=tags")?);
    let root = Store::directory(worker)?;
    let _sample = Store::delivery_at(&root.join("tag-events"), true)?;
    loop {
        let mut paths = fs::read_dir(root.join("tag-events"))?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
        if paths.is_empty() {
            return Ok(());
        }
        let state = Store::read(worker)?
            .map(Ok)
            .unwrap_or_else(|| State::new(crate::relay_store::endpoint(worker)?, &home))?;
        let refs = match read_refs(&home, &state) {
            Ok(current) => current,
            Err(error) if error.to_string().starts_with("REVIEW_WORKTREE_PENDING") => return Ok(()),
            Err(error) => return Err(error),
        };
        let started = earliest(&paths)?;
        let start = |head: &str| Ok((committed_ms(&home, &state, head), started));
        let accepted = sample_head(&root, &refs, start, |head| {
            super::events::enqueue(
                worker,
                json!({"kind":"native_head","review_class":"commit","head":head,"source":"controller Git metadata","at_ms":kah_observer::session::now_ms()}),
            )
        })?;
        sample_tags(&root, &refs, accepted, |added, initial| {
            super::events::enqueue(
                worker,
                json!({"kind":"native_git_tag","tags":added,"source":"controller Git metadata","initial_sample":initial,"at_ms":kah_observer::session::now_ms()}),
            )
        })?;
        for path in paths {
            fs::remove_file(path)?
        }
    }
}
/// A background startup sample can race the first tool. When the start HEAD
/// is not accepted, review existing tags once on an unobserved session rather
/// than silently losing that first tag.
pub fn observed_additions(previous: Option<&[String]>, current: &[String]) -> Vec<String> {
    additions(previous.unwrap_or_default(), current)
}
pub fn additions(previous: &[String], current: &[String]) -> Vec<String> {
    current
        .iter()
        .filter(|r| !previous.contains(r))
        .cloned()
        .collect()
}
/// git/git v2.53.0 Documentation/git-for-each-ref.adoc: --include-root-refs
/// includes HEAD, including detached HEAD, in the same native metadata read.
pub fn head(home: &Path, state: &State) -> Result<Option<String>> {
    Ok(read_refs(home, state)?
        .into_iter()
        .find_map(|r| r.strip_prefix("HEAD ").map(str::to_owned)))
}
/// Compare the first sample with the session-start baseline
/// (session_baseline::accept; `start` is its fallback). Later samples review
/// changed HEADs. `review` enqueues the sampled commit. Returns true when this
/// first sample accepted HEAD as the session-start baseline.
pub fn sample_head(
    root: &Path,
    refs: &[String],
    start: impl FnOnce(&str) -> Result<(Option<u64>, Option<u64>)>,
    review: impl FnOnce(&str) -> Result<()>,
) -> Result<bool> {
    let head = refs.iter().find_map(|r| r.strip_prefix("HEAD "));
    let previous: Option<Option<String>> = super::state::read(&root.join("git-head.json"))?;
    let first = previous.is_none();
    let changed = match head {
        None => false,
        Some(head) if first => !super::session_baseline::accept(root, head, start)?,
        Some(head) => previous.flatten().as_deref() != Some(head),
    };
    if let Some(head) = head.filter(|_| changed) {
        review(head)?;
    }
    if first || changed {
        crate::unattended_report::save(root, "git-head.json", &json!(head))?;
    }
    Ok(first && !changed)
}
/// `review` gets the new or retargeted tags and whether this is the first
/// sample. `accepted` is the result of sample_head for the same refs: at an
/// accepted session-start HEAD the tags are the baseline too, because no worker
/// change exists for a checkpoint to judge.
pub fn sample_tags(
    root: &Path,
    refs: &[String],
    accepted: bool,
    review: impl FnOnce(Vec<String>, bool) -> Result<()>,
) -> Result<()> {
    let current = refs
        .iter()
        .filter(|r| r.starts_with("refs/tags/"))
        .cloned()
        .collect::<Vec<_>>();
    let prior: Option<Vec<String>> = super::state::read(&root.join("git-tags.json"))?;
    let added = if accepted {
        vec![]
    } else {
        observed_additions(prior.as_deref(), &current)
    };
    if !added.is_empty() {
        review(added, prior.is_none())?;
    }
    crate::unattended_report::save(root, "git-tags.json", &json!(current))
}
/// The earliest pending sample request: the session start for `start`.
fn earliest(paths: &[PathBuf]) -> Result<Option<u64>> {
    let mut times = vec![];
    for path in paths {
        let event: Option<Value> = super::state::read(path)?;
        times.extend(event.and_then(|e| e["at_ms"].as_u64()));
    }
    Ok(times.into_iter().min())
}
fn read_refs(home: &Path, state: &State) -> Result<Vec<String>> {
    let args = [
        "for-each-ref",
        "--include-root-refs",
        "--format=%(refname) %(objectname)",
        "refs/tags",
        "HEAD",
    ];
    Ok(git(home, state, &args)?
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect())
}
/// When `head` was committed, in ms (second precision); `None` when unreadable.
pub fn committed_ms(home: &Path, state: &State, head: &str) -> Option<u64> {
    git(home, state, &["show", "-s", "--format=%ct", head, "--"])
        .ok()
        .flatten()
        .and_then(|t| t.trim().parse::<u64>().ok())
        .map(|s| s.saturating_mul(1000))
}
/// Read-only Git metadata. `None` when the project is not a Git repository.
fn git(home: &Path, state: &State, args: &[&str]) -> Result<Option<String>> {
    let mut command = vec![
        "git",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
    ];
    command.extend_from_slice(args);
    let output = if state.worker.project.starts_with(home.join("repos/mine")) {
        Command::new(command[0])
            .args(&command[1..])
            .current_dir(&state.worker.project)
            .output()?
    } else {
        let project = kah_sandbox::Project {
            domain: kah_sandbox::Domain::Research,
            root: state.worker.project.clone(),
            relative_cwd: Default::default(),
            needs_creation: false,
        };
        let git = super::capture::worktree_git(state)?;
        let executor =
            crate::native_setup::executor(home, &project, Default::default(), git.as_deref())?
                .read_only_project();
        let command = command.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
        kah_sandbox::output_retrying_start(&mut executor.command(&project.root, &command)?)?
    };
    if !output.status.success() {
        if !state.worker.project.join(".git").exists() {
            return Ok(None);
        }
        return Err("REVIEW_GIT_TAGS stage=tags: Git metadata could not be read".into());
    }
    Ok(Some(String::from_utf8(output.stdout)?))
}
