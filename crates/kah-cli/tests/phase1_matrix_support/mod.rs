use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
mod atomize;
mod checkpoint;
mod confirmation;
mod current_rules;
mod fixture;
mod journey;
mod option1;
mod option1_budget;
mod real;
mod selection;
pub(super) mod usage;
pub fn launch() {
    assert_eq!(std::env::var("KAH_PHASE1_MAKE_GATE").as_deref(), Ok("1"));
    let fixture = fixture::Fixture::new();
    save(&fixture.evidence.join("native-versions.json"), &versions());
    drive(fixture);
}
pub fn confirmation_replay() {
    assert_eq!(std::env::var("KAH_PHASE1_MAKE_GATE").as_deref(), Ok("1"));
    let original = PathBuf::from(std::env::var_os("KAH_PHASE1_NATIVE_ROOT").unwrap());
    let mut f = fixture::Fixture::read(&original);
    if std::env::var_os("KAH_PHASE1_NATIVE_PROJECT").is_some() {
        f.corpus = PathBuf::from(std::env::var_os("KAH_PHASE1_CORPUS").unwrap());
        f.project = PathBuf::from(std::env::var_os("KAH_PHASE1_NATIVE_PROJECT").unwrap());
        let head = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(&f.project)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert!(head.status.success());
        assert_eq!(
            String::from_utf8(head.stdout).unwrap().trim(),
            "cd415f4066df4da06751303ca9d27756c755ff9f"
        );
        assert!(
            !f.project
                .join("docs/observer-phase1/decisive-plants")
                .exists()
        );
    }
    f.name = format!(
        "kah-phase1-confirmation-{}",
        kah_observer::session::now_ms()
    );
    let root = PathBuf::from("/tmp").join(&f.name);
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    fs::copy(f.root.join("herdr.toml"), root.join("herdr.toml")).unwrap();
    f.root = root;
    let protected = fs::read(f.evidence.join("global-config-before.json")).unwrap();
    f.evidence = PathBuf::from(std::env::var_os("KAH_PHASE1_CONFIRM_EVIDENCE").unwrap());
    kah_core::run_store::ensure_private_directory(&f.evidence).unwrap();
    fs::write(f.evidence.join("global-config-before.json"), protected).unwrap();
    save(
        &f.root.join("fixture.json"),
        &serde_json::to_value(&f).unwrap(),
    );
    save(
        &f.evidence.join("ownership.json"),
        &serde_json::to_value(&f).unwrap(),
    );
    fs::copy(
        original.join("fixture.json"),
        f.evidence.join("original-fixture.json"),
    )
    .unwrap();
    // Same private native HOME/transcript, new Herdr control session only.
    drive(f);
}
fn drive(fixture: fixture::Fixture) {
    let command = format!(
        "HERDR_CONFIG_PATH={} herdr --session {}",
        crate::pty::quote(fixture.root.join("herdr.toml").to_str().unwrap()),
        crate::pty::quote(&fixture.name)
    );
    let mut terminal = crate::pty::Terminal::start_sized(
        &command,
        &fixture.evidence.join("terminal.log"),
        180,
        45,
    );
    terminal.wait_for("KAH_PHASE1_READY");
    terminal.submit(&format!(
        "KAH_PHASE1_ROOT={} {} --exact matrix_entrypoint --nocapture > {} 2>&1",
        crate::pty::quote(fixture.root.to_str().unwrap()),
        crate::pty::quote(std::env::current_exe().unwrap().to_str().unwrap()),
        crate::pty::quote(fixture.evidence.join("helper.log").to_str().unwrap())
    ));
    loop {
        terminal.respond_to_cursor_queries();
        if fixture.evidence.join("result.json").exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    // Herdr0.9.3 skill: named session stop affects only this private socket.
    let stop = Command::new("herdr")
        .args(["session", "stop", &fixture.name, "--json"])
        .output()
        .unwrap();
    assert!(stop.status.success());
    drop(terminal);
    if matches!(
        std::env::var("KAH_PHASE1_REPLAY_STAGE").as_deref(),
        Ok("option1-e2e" | "current-rules-e2e")
    ) {
        let key = fixture.home.join(".config/kah/typesafe.key");
        if key.exists() {
            fs::remove_file(key).unwrap();
        }
    }
    let before = fixture.evidence.join("global-config-before.json");
    let protected: std::collections::BTreeMap<String, String> = if before.exists() {
        serde_json::from_slice(&fs::read(before).unwrap()).unwrap()
    } else {
        Default::default()
    };
    for (path, digest) in protected {
        assert_eq!(
            kah_observer::commit_check::digest(&fs::read(path).unwrap()),
            digest,
            "global native config changed"
        );
    }
    let result: Value =
        serde_json::from_slice(&fs::read(fixture.evidence.join("result.json")).unwrap()).unwrap();
    assert_eq!(result["passed"], true, "{result}");
}
pub fn entry() {
    let root = PathBuf::from(std::env::var_os("KAH_PHASE1_ROOT").unwrap());
    let fixture = fixture::Fixture::read(&root);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if std::env::var("KAH_PHASE1_REPLAY_STAGE").as_deref() == Ok("current-rules-e2e") {
            current_rules::run(&fixture)
        } else if std::env::var("KAH_PHASE1_REPLAY_STAGE").as_deref() == Ok("option1-e2e") {
            option1::run(&fixture)
        } else if matches!(
            std::env::var("KAH_PHASE1_REPLAY_STAGE").as_deref(),
            Ok("atomize" | "atomize-refine")
        ) {
            atomize::run(&fixture)
        } else if std::env::var_os("KAH_OPTION1_CHECKPOINT_INPUT").is_some() {
            checkpoint::run(&fixture)
        } else if std::env::var_os("KAH_PHASE1_SELECTION_JOBS").is_some() {
            selection::run(&fixture)
        } else if std::env::var_os("KAH_PHASE1_JEV_OUTPUT").is_some() {
            real::run(&fixture)
        } else {
            journey::run(&fixture)
        }
    }));
    save(
        &fixture.evidence.join("result.json"),
        &json!({"passed":result.is_ok(),"error":result.err().map(|p|p.downcast_ref::<String>().cloned().or_else(||p.downcast_ref::<&str>().map(|s|s.to_string())).unwrap_or_else(||"panic".into())),"zero_jev":!matches!(std::env::var("KAH_PHASE1_REPLAY_STAGE").as_deref(),Ok("option1-e2e"|"current-rules-e2e")),"root":root}),
    );
}
pub fn save(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn versions() -> Value {
    let versions: std::collections::BTreeMap<_, _> = ["codex", "claude", "herdr"]
        .into_iter()
        .map(|name| {
            let output = Command::new(name).arg("--version").output().unwrap();
            assert!(output.status.success());
            (
                name,
                String::from_utf8(output.stdout).unwrap().trim().to_owned(),
            )
        })
        .collect();
    json!(versions)
}
pub fn herdr(args: &[&str]) -> Value {
    assert_eq!(std::env::var("HERDR_ENV").as_deref(), Ok("1"));
    let out = Command::new("herdr").args(args).output().unwrap();
    assert!(
        out.status.success(),
        "HERDR_COMMAND: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    if args.starts_with(&["pane", "run"]) && out.stdout.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|error| panic!("PHASE1_HERDR_JSON stage=harness: {args:?}: {error}"))
}
