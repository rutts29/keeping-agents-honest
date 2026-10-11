use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
mod confirmation;
mod fixture;
mod journey;
mod usage;
pub fn launch() {
    assert_eq!(std::env::var("KAH_PHASE1_MAKE_GATE").as_deref(), Ok("1"));
    let fixture = fixture::Fixture::new();
    save(&fixture.evidence.join("native-versions.json"), &versions());
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
    let protected: std::collections::BTreeMap<String, String> = serde_json::from_slice(
        &fs::read(fixture.evidence.join("global-config-before.json")).unwrap(),
    )
    .unwrap();
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
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| journey::run(&fixture)));
    save(
        &fixture.evidence.join("result.json"),
        &json!({"passed":result.is_ok(),"error":result.err().map(|p|p.downcast_ref::<String>().cloned().or_else(||p.downcast_ref::<&str>().map(|s|s.to_string())).unwrap_or_else(||"panic".into())),"zero_jev":true,"root":root}),
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
