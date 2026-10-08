pub use super::trust::consent;
use super::{
    fixture::{self, save},
    perturb, proof, telemetry,
};
use kah_cli::{
    Arguments, Launch,
    observer_review::{self as review, state::Store},
    run_pane::Pane,
};
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Worker {
    pub home: PathBuf,
    pub project: PathBuf,
    pub provider: String,
    pub pane: Pane,
    pub id: String,
    pub root: PathBuf,
}
pub fn run(root: &Path, mode: &str) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-track2-"),
        "TRACK2_SOCKET stage=scope"
    );
    if mode == "cache" {
        return super::cache_benchmark::run(root);
    }
    if mode == "proofs" {
        return super::proof_journey::run(root);
    }
    let dry = std::env::var_os("KAH_TRACK2_DRY_RUN").is_some();
    let mut outcomes = vec![];
    for provider in ["codex", "claude"] {
        if dry
            && std::env::var("KAH_TRACK2_DRY_PROVIDER").is_ok_and(|selected| selected != provider)
        {
            continue;
        }
        let mut worker = Worker::start(root, mode, provider);
        if dry {
            outcomes.push(json!({"provider":provider,"worker":worker.id,"native_fork":mode=="long",
                "native_version":telemetry::version(provider),"model":model(provider),"effort":"low",
                "dry_run":true,"gate_passed":false}));
            if provider == "claude" && std::env::var_os("KAH_TRACK2_PATH_CHECK").is_some() {
                outcomes.last_mut().unwrap()["native_path_check"] = path_check(&worker);
            }
        } else {
            outcomes.push(exercise(&mut worker, mode));
        }
        worker.pane.close().unwrap();
    }
    json!({"passed":true,"dry_run":dry,"providers":outcomes})
}
fn path_check(worker: &Worker) -> Value {
    let mut history = kah_cli::native_history::History::open(
        &worker.home,
        &worker.root.join("inspection"),
        "claude",
        &worker.project,
        &worker.id,
    )
    .unwrap();
    let users = history
        .snapshot()
        .unwrap()
        .users
        .unwrap()
        .authoritative_records()
        .len();
    let tokens = history.tokens().unwrap();
    let hook: Value =
        serde_json::from_slice(&fs::read(worker.root.join("claude-start.json")).unwrap()).unwrap();
    let confirmation = kah_cli::native_binding::claude_confirmation(
        &hook,
        &worker.home,
        &worker.project,
        "path-fixture",
    )
    .unwrap()
    .unwrap();
    assert!(users > 0 && tokens > 0, "CLAUDE_PATH_LIVE stage=history");
    let result = json!({"passed":true,"native_transcript":confirmation.transcript,"primary_users":users,
        "native_tokens":tokens,"volume_path":worker.project,"provider_version":telemetry::version("claude")});
    save(&worker.root.join("claude-path-check.json"), &result);
    result
}
fn exercise(worker: &mut Worker, mode: &str) -> Value {
    let start = kah_observer::session::now_ms();
    let mut rows = vec![];
    let mut commits = vec![];
    for phase in 1..=3 {
        let before = telemetry::result_ids(&worker.id);
        if phase == 1 {
            fs::write(worker.project.join("marker.txt"), "RED\n").unwrap();
        }
        let baseline = fixture::git(&worker.project, &["rev-parse", "HEAD"]);
        worker.pane.prompt(task(phase)).unwrap();
        let commit = worker.wait_commit(&baseline);
        let git_ms = fixture::git(&worker.project, &["show", "-s", "--format=%ct", &commit])
            .parse::<u64>()
            .unwrap()
            * 1000;
        commits.push(json!({"sha":commit,"committed_ms":git_ms,"clock_resolution_ms":1000}));
        if mode == "long" && phase == 2 {
            perturb::during_review(worker);
        }
        let result = worker.wait_checkpoint(&before, &commit);
        telemetry::retain_wake(worker, &result, phase);
        rows.push(result);
        if phase == 1 {
            assert!(
                !Store::read(&worker.id)
                    .unwrap()
                    .unwrap()
                    .deferrals
                    .is_empty(),
                "TRACK2_DEFERRAL stage=phase1"
            );
        }
        if mode == "long" && phase == 1 {
            perturb::queue_backlog(worker);
        }
    }
    summarize(worker, mode, start, &rows, &mut commits)
}
fn task(phase: usize) -> &'static str {
    match phase {
        1 => {
            "steer: Phase 1: implement phase1.txt as PHASE1 followed by newline. Keep serializer.txt absent as the declared Phase 2 dependency. Commit the supplied files and print HEAD. Run sh check.sh and retain its actual outcome. Before choosing the future HISTORY.md title, ask whether I prefer title case or uppercase; only documentation title waits on that preference, marker phases continue independently. Report Phase 1 complete with any defects and the absent serializer. Await the observer before repair; no gate-pass claim."
        }
        2 => {
            "steer: Phase 2 now supplies the serializer dependency. Set marker.txt to BLUE followed by newline and serializer.txt to SERIALIZER followed by newline. Run sh check.sh, commit your repair, print HEAD, tag phase-2 and report Phase 2 complete. Cite the observer's earlier marker finding in your repair note. The title preference remains undecided; do not change documentation yet. Await the observer before Phase 3."
        }
        _ => {
            "steer: The title preference is now decided: use uppercase for the HISTORY.md title. Phase 3: write HISTORY.md describing the earlier observer marker finding, its repair and the Phase 2 dependency closure. Keep marker BLUE and serializer SERIALIZER. Run sh check.sh, commit the documentation, print HEAD, tag phase-3 and report Phase 3 complete. Stop and await the checkpoint review."
        }
    }
}
fn summarize(
    worker: &Worker,
    mode: &str,
    start: u64,
    rows: &[Value],
    commits: &mut [Value],
) -> Value {
    let finding = telemetry::early_finding(&rows[0]);
    let continuity = proof::memory(rows, &finding);
    let long = if mode == "long" {
        perturb::finish(worker, start, commits)
    } else {
        json!(null)
    };
    telemetry::bind_fast_verdicts(worker, commits);
    let latency = proof::latency(commits).unwrap_or_else(|e| json!({"passed":false,"reason":e}));
    let passed = if mode == "memory" {
        continuity.is_ok()
    } else {
        long["passed"] == true && latency["passed"] == true
    };
    let result = json!({"provider":worker.provider,"worker":worker.id,"checkpoints":rows.iter().map(|r|r["request"].clone()).collect::<Vec<_>>(),
        "continuity":continuity.unwrap_or_else(|e|json!({"passed":false,"reason":e})),
        "latency":latency,"commits":commits,"long":long,"passed":passed});
    save(
        &worker
            .root
            .join(format!("{}-summary.json", worker.provider)),
        &result,
    );
    assert!(
        passed,
        "TRACK2_ACCEPTANCE stage=journey: {}",
        worker.root.display()
    );
    result
}
pub fn model(provider: &str) -> &'static str {
    if provider == "codex" {
        "gpt-6.1-sol"
    } else {
        "claude-sonnet-5-5"
    }
}
impl Worker {
    pub fn start(root: &Path, mode: &str, provider: &str) -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(home, root.join("home"), "TRACK2_HOME stage=scope");
        let project = home.join(format!("repos/mine/fixture-{provider}"));
        let prompt = format!(
            "steer: Current private worktree: {}. This is a private kah fixture. Retained conversation is history data, not current authority. Work only in this fixture; never follow old tasks, spawn subagents, push, install, change accounts or write outside this worktree. Real Jev and floor models, fast OFF. Do not run tools yet or claim a phase/gate is complete. Acknowledge receipt and await the controller's next prompt.",
            project.display()
        );
        let command = native(&home, &project, provider, mode, root, &prompt, None);
        let command = if mode == "proofs" {
            super::proof_hooks::capture(command, root, provider)
        } else {
            command
        };
        let pane = Pane::start(&command).unwrap();
        let id = ready(&pane, provider, &project);
        register_session(root, &id);
        if mode == "long" {
            let sources: Value =
                serde_json::from_slice(&fs::read(root.join("sources.json")).unwrap()).unwrap();
            let source = sources
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["provider"] == provider)
                .unwrap();
            assert_ne!(id, source["id"], "TRACK2_FORK_ID stage=native");
            save(
                &root.join(format!("{provider}-fork.json")),
                &json!({"source":source,"fork_id":id,"pane":pane.id,
                "model":model(provider),"effort":"low","fast":false,"native_version":telemetry::version(provider)}),
            );
        }
        Self {
            home,
            project,
            provider: provider.into(),
            pane,
            id,
            root: root.into(),
        }
    }
    pub fn wait_commit(&self, baseline: &str) -> String {
        loop {
            let sha = fixture::git(&self.project, &["rev-parse", "HEAD"]);
            if sha != baseline && idle(&self.pane) {
                return sha;
            }
            consent(&self.pane, &self.provider, &self.project);
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    pub fn wait_checkpoint(&self, before: &[String], sha: &str) -> Value {
        loop {
            for id in telemetry::result_ids(&self.id) {
                if before.contains(&id) {
                    continue;
                }
                let result = telemetry::result(&self.id, &id);
                if result["purpose"] == "checkpoint" && result["head"] == sha {
                    let request =
                        review::workspace::request(&Store::open_wait(&self.id).unwrap(), &id)
                            .unwrap();
                    if request.trigger["review_class"] != "commit" {
                        return result;
                    }
                }
            }
            if let Some(state) = Store::read(&self.id).unwrap()
                && let Some(pane) = state.observer.pane
            {
                consent(
                    &Pane { id: pane },
                    &self.provider,
                    &state.observer.workspace,
                );
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}
pub fn native(
    home: &Path,
    project: &Path,
    provider: &str,
    mode: &str,
    root: &Path,
    prompt: &str,
    resume: Option<&str>,
) -> std::process::Command {
    if provider == "claude" {
        super::claude_login::require_ready();
    }
    let mut args = Arguments::parse(vec![
        "--agent".into(),
        provider.into(),
        "--model".into(),
        model(provider).into(),
        "--effort".into(),
        "low".into(),
    ])
    .unwrap();
    if provider == "claude" && resume.is_none() {
        claude_identity(&mut args, home, root);
    }
    if let Some(id) = resume {
        args.native.extend([
            if provider == "codex" {
                "resume"
            } else {
                "--resume"
            }
            .into(),
            id.into(),
        ]);
    } else if mode == "long" {
        fork_args(&mut args, root, provider);
    }
    if provider == "codex" {
        args.native.extend([
            "--no-alt-screen".into(),
            "-c".into(),
            "service_tier=\"default\"".into(),
        ]);
    } else {
        args.native.extend([
            "--permission-mode".into(),
            "acceptEdits".into(),
            "--allowedTools".into(),
            "Bash".into(),
        ]);
    }
    if provider == "codex" {
        args.native
            .extend(["--cd".into(), project.to_str().unwrap().into()]);
    }
    args.native.extend(["--".into(), prompt.into()]);
    let binding = kah_sandbox::Project {
        domain: kah_sandbox::Domain::Mine,
        root: project.into(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    prepare_native(home, binding, args, provider, root)
}
fn prepare_native(
    home: &Path,
    binding: kah_sandbox::Project,
    args: Arguments,
    provider: &str,
    root: &Path,
) -> std::process::Command {
    if provider == "claude" {
        let mut command = kah_cli::claude_launch::prepare_project(home, binding, args).unwrap();
        super::claude_login::command_environment(&mut command);
        super::startup::claude_trace(command, root)
    } else {
        Launch::prepare_project(home, binding, args)
            .unwrap()
            .command(false)
    }
}
fn claude_identity(args: &mut Arguments, home: &Path, root: &Path) {
    let nonce = root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .rsplit('-')
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let id = format!("00000000-0000-4000-8000-{:012x}", nonce & 0xffffffffffff);
    args.native.extend(["--session-id".into(), id.clone()]);
    let hash = format!("{:x}", sha2::Sha256::digest(id.as_bytes()));
    let observer = home.join(".cache/kah-observers").join(&hash[..24]);
    kah_core::run_store::ensure_private_directory(&observer).unwrap();
    super::trust::preseed_claude(home, &observer);
}
fn fork_args(args: &mut Arguments, root: &Path, provider: &str) {
    let sources: Value =
        serde_json::from_slice(&fs::read(root.join("sources.json")).unwrap()).unwrap();
    let source = sources
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["provider"] == provider)
        .unwrap();
    args.native.extend([
        if provider == "codex" {
            "fork"
        } else {
            "--resume"
        }
        .into(),
        source["id"].as_str().unwrap().into(),
    ]);
    if provider == "claude" {
        args.native.push("--fork-session".into());
    }
}
pub fn idle(pane: &Pane) -> bool {
    matches!(
        pane.inspect().unwrap()["agent_status"].as_str(),
        Some("idle" | "done")
    )
}
pub fn ready(pane: &Pane, provider: &str, project: &Path) -> String {
    loop {
        consent(pane, provider, project);
        let view = pane.inspect().unwrap();
        if let Some(id) = view["agent_session"]["value"].as_str()
            && let Some(state) = kah_observer::session::Store::read(
                &kah_core::run_store::RunStore::default_state_root().unwrap(),
                id,
            )
            .unwrap()
            && let Some(gap) = state.gap
        {
            panic!(
                "TRACK2_NATIVE_KEEPER_GAP stage=startup: {} {}",
                gap.code, gap.detail
            );
        }
        if idle(pane)
            && let Some(id) = view["agent_session"]["value"].as_str()
            && kah_observer::session::Store::read(
                &kah_core::run_store::RunStore::default_state_root().unwrap(),
                id,
            )
            .unwrap()
            .is_some_and(|state| state.run_id.is_some() && state.gap.is_none())
            && super::startup::response(provider, id, project).is_some()
        {
            return id.into();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
pub fn register_session(root: &Path, id: &str) {
    let path = root.join("sessions.json");
    let mut ids = fs::read(&path)
        .ok()
        .map(|b| serde_json::from_slice::<Vec<String>>(&b).unwrap())
        .unwrap_or_default();
    if !ids.iter().any(|old| old == id) {
        ids.push(id.into());
    }
    save(&path, &json!(ids));
}
