//! Private volume fixture setup; the frozen task/check and product controller stay unchanged.
use super::*;
use kah_observer::{judge::routing::Backend, session::Binding};
use sha2::{Digest, Sha256};

pub fn launch(home: &Path, project: &Path, provider: &str, task: &str, verify: &str) -> String {
    if provider == "claude" {
        crate::track2_support::claude_login_ready();
    }
    let id = create(home, project, provider, task, verify);
    let output = Command::new(home.join(".local/bin/kah"))
        .args(["resume", &id])
        .current_dir(project)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "TRACK2_PRIVATE_RUN stage=launch: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    format!(
        "Created {id}\n{}",
        String::from_utf8(output.stdout).unwrap()
    )
}
fn create(home: &Path, project: &Path, provider: &str, task: &str, verify: &str) -> String {
    let root = PathBuf::from(std::env::var_os("KAH_TRACK2_ROOT").unwrap());
    assert_eq!(home, root.join("home"));
    let id = format!(
        "run-track2-{}-{}",
        kah_observer::session::now_ms(),
        std::process::id()
    );
    let tree = volume_tree(home, project, &id);
    let mut control = kah_core::unattended::RunControl::new(task, Default::default()).unwrap();
    control.checkpoint(tree.base_commit.clone()).unwrap();
    let mut native = kah_run::native::Native::new(provider).unwrap();
    native.name = Some(kah_run::native::session_name(&id, task));
    if provider == "codex" {
        native.codex_config.push("service_tier=\"fast\"".into());
    }
    let record = kah_run::store::Record {
        native: Some(native),
        project: project.into(),
        domain: "mine".into(),
        worktree: tree.path.clone(),
        model: Some(runs::fixture_model(provider).into()),
        effort: Some("low".into()),
        control,
    };
    let base = RunStore::default_state_root().unwrap().join("runs");
    let store = Runs::create(&base, &id, record).unwrap();
    tree.save(&store.directory().join("worktree.json")).unwrap();
    save_settings(home, &tree, &id, provider, verify, store.directory());
    kah_cli::unattended_report::save(store.directory(), "dispatch.json", &Value::Null).unwrap();
    if provider == "claude" {
        crate::track2_support::preseed_claude(home, &tree.path);
    }
    drop(store);
    id
}
fn volume_tree(home: &Path, project: &Path, id: &str) -> kah_run::worktree::Worktree {
    let workspace = home.join("run-workspaces");
    kah_core::run_store::ensure_private_directory(&workspace).unwrap();
    kah_run::access::traverse(&workspace, kah_run::access::service_uid().unwrap()).unwrap();
    let tree = kah_run::worktree::Worktree::create(project, &workspace.join(id), id).unwrap();
    kah_run::access::traverse(
        tree.path.parent().unwrap(),
        kah_run::access::service_uid().unwrap(),
    )
    .unwrap();
    kah_run::access::share_tree(&tree.path, kah_run::access::service_uid().unwrap(), true).unwrap();
    kah_run::access::share_tree(
        &tree.git_dir,
        kah_run::access::service_uid().unwrap(),
        false,
    )
    .unwrap();
    tree
}
fn save_settings(
    home: &Path,
    tree: &kah_run::worktree::Worktree,
    id: &str,
    provider: &str,
    verify: &str,
    directory: &Path,
) {
    let settings = kah_cli::unattended_observer::Settings {
        test: verify.into(),
        allow_hosts: vec![],
        observer: Binding {
            session: id.into(),
            project: tree.path.clone(),
            codex_home: home.join(".codex"),
            native: home.join(".local/bin/codex").canonicalize().unwrap(),
            model: runs::fixture_model(provider).into(),
            effort: "low".into(),
            timeout_secs: 45,
            judge: Backend::Jev,
            jev_model: "jev-latest".into(),
        },
    };
    kah_cli::unattended_report::save(
        directory,
        "settings.json",
        &serde_json::to_value(settings).unwrap(),
    )
    .unwrap();
}

pub fn dialogs(home: &Path, record: &kah_run::store::Record, provider: &str) {
    let Some(native) = &record.native else {
        return;
    };
    if let Some(pane) = &native.pane {
        crate::track2_support::private_consent(
            &Pane { id: pane.clone() },
            provider,
            &record.worktree,
        );
    }
    let Some(worker) = &native.session else {
        return;
    };
    if provider == "claude" {
        let hash = format!("{:x}", Sha256::digest(worker));
        let path = home.join(".cache/kah-observers").join(&hash[..24]);
        let config: Value =
            serde_json::from_slice(&fs::read(kah_claude::paths::global_config(home)).unwrap())
                .unwrap();
        // Native startup publishes the worker ID before a review creates its workspace.
        // Trust only the fixture-owned directory after it exists; never create it here.
        if path.is_dir()
            && config["projects"][path.to_str().unwrap()]["hasTrustDialogAccepted"] != true
        {
            crate::track2_support::preseed_claude(home, &path);
        }
    }
    if let Some(state) = Reviews::read(worker).unwrap()
        && let Some(pane) = state.observer.pane
    {
        crate::track2_support::private_consent(
            &Pane { id: pane },
            provider,
            &state.observer.workspace,
        );
    }
}
pub fn courier(pane: &Pane, project: &Path) -> String {
    loop {
        crate::track2_support::private_consent(pane, "codex", project);
        let view = pane.inspect().unwrap();
        if let Some(id) = view["agent_session"]["value"].as_str()
            && let Some(state) = Store::read(&RunStore::default_state_root().unwrap(), id).unwrap()
            && let Some(gap) = state.gap
        {
            panic!("TRACK2_COURIER_GAP stage=startup: {}", gap.detail);
        }
        if matches!(view["agent_status"].as_str(), Some("idle" | "done"))
            && let Some(id) = view["agent_session"]["value"].as_str()
            && Store::read(&RunStore::default_state_root().unwrap(), id)
                .unwrap()
                .is_some_and(|s| s.run_id.is_some() && s.gap.is_none())
        {
            return id.into();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
pub fn rounds(directory: &Path) -> Value {
    let rows = fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            (entry.path().extension().is_some_and(|e| e == "json")).then(|| {
                kah_cli::observer_review::source_store::read::<Value>(&entry.path()).unwrap()
            })
        })
        .collect::<Vec<_>>();
    let mut scopes = std::collections::BTreeSet::new();
    for row in &rows {
        let binding = &row["binding"];
        let scope = (
            binding["worker"].as_str().unwrap(),
            binding["head"].as_str().unwrap(),
            binding["intent_revision"].as_u64().unwrap(),
        );
        assert!(
            scopes.insert(scope),
            "TRACK2_MULTIPLE_VERIFICATION_ROUNDS stage=verification"
        );
    }
    json!({"passed":true,"scope":"one round per worker/SHA/intent revision","records":rows})
}
pub fn stopped(id: &str, provider: &str, record: &kah_run::store::Record) -> Value {
    let directory = Runs::path(&RunStore::default_state_root().unwrap().join("runs"), id).unwrap();
    let report = fs::read(directory.join("report.json"))
        .ok()
        .map(|b| serde_json::from_slice::<Value>(&b).unwrap());
    json!({"passed":false,"early_run":true,"final_acceptance":false,"run":id,"provider":provider,
        "status":record.published_status(),"controller_report":report,"record":record,"trace":directory})
}
