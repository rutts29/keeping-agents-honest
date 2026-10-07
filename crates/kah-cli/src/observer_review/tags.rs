//! Event-driven Git metadata sampling; never a host Git call in Research.
use super::{
    hook,
    state::{State, Store},
};
use crate::Result;
use serde_json::json;
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
        sample_head(worker, &root, &refs)?;
        let current = refs
            .into_iter()
            .filter(|r| r.starts_with("refs/tags/"))
            .collect::<Vec<_>>();
        let prior: Option<Vec<String>> = super::state::read(&root.join("git-tags.json"))?;
        let added = observed_additions(prior.as_deref(), &current);
        if !added.is_empty() {
            super::events::enqueue(
                worker,
                json!({"kind":"native_git_tag","tags":added,"source":"controller Git metadata","initial_sample":prior.is_none(),"at_ms":kah_observer::session::now_ms()}),
            )?;
        }
        crate::unattended_report::save(&root, "git-tags.json", &json!(current))?;
        for path in paths {
            fs::remove_file(path)?
        }
    }
}
/// A background startup sample can race the first tool. Review existing tags
/// once on an unobserved session rather than silently losing that first tag.
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
fn sample_head(worker: &str, root: &Path, refs: &[String]) -> Result<()> {
    let head = refs.iter().find_map(|r| r.strip_prefix("HEAD "));
    let previous: Option<String> = super::state::read(&root.join("git-head.json"))?;
    if let Some(head) = head
        && previous.as_deref() != Some(head)
    {
        super::events::enqueue(
            worker,
            json!({"kind":"native_head","review_class":"commit","head":head,"source":"controller Git metadata","at_ms":kah_observer::session::now_ms()}),
        )?;
        crate::unattended_report::save(root, "git-head.json", &json!(head))?;
    }
    Ok(())
}
fn read_refs(home: &Path, state: &State) -> Result<Vec<String>> {
    let args = [
        "git",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "for-each-ref",
        "--include-root-refs",
        "--format=%(refname) %(objectname)",
        "refs/tags",
        "HEAD",
    ];
    let output = if state.worker.project.starts_with(home.join("repos/mine")) {
        Command::new(args[0])
            .args(&args[1..])
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
        executor
            .command(&project.root, &args.map(str::to_owned))?
            .output()?
    };
    if !output.status.success() {
        if !state.worker.project.join(".git").exists() {
            return Ok(vec![]);
        }
        return Err("REVIEW_GIT_TAGS stage=tags: Git metadata could not be read".into());
    }
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .map(str::to_owned)
        .collect())
}
