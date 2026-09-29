use super::start as start_client;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, symlink},
    path::PathBuf,
    time::SystemTime,
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn verify() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let global = home.join(".codex/config.toml");
    let original = fs::read(&global).unwrap();
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = Fixture(home.join(format!(".local/state/kah/probes/trust-{nonce}")));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&fixture.0)
        .unwrap();
    let project = fixture.0.join("project");
    fs::create_dir(&project).unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .arg(&project)
            .status()
            .unwrap()
            .success()
    );
    let fake_global = fixture.0.join("pretend-global.toml");
    fs::write(&fake_global, "[features]\nhooks = false\napps = false\n").unwrap();
    let overlay = fixture.0.join("overlay");
    fs::create_dir(&overlay).unwrap();
    symlink(&fake_global, overlay.join("config.toml")).unwrap();
    symlink(home.join(".codex/auth.json"), overlay.join("auth.json")).unwrap();
    start(&overlay, &project, &fixture.0.join("before.stderr"));
    let reproduced = fs::read_to_string(&fake_global)
        .unwrap()
        .contains("trust_level = \"trusted\"");
    assert!(
        reproduced,
        "symlink write-through regression was not reproduced"
    );
    fs::remove_file(overlay.join("config.toml")).unwrap();
    let config = format!(
        "[features]\nhooks=false\napps=false\n[projects.{}]\ntrust_level=\"untrusted\"\n",
        serde_json::to_string(&project).unwrap()
    );
    fs::write(overlay.join("config.toml"), &config).unwrap();
    let fake_before = fs::read(&fake_global).unwrap();
    start(&overlay, &project, &fixture.0.join("after.stderr"));
    assert_eq!(fs::read(&fake_global).unwrap(), fake_before);
    assert_eq!(
        fs::read_to_string(overlay.join("config.toml")).unwrap(),
        config
    );
    assert_eq!(fs::read(&global).unwrap(), original);
    let evidence = json!({"symlinkWriteThroughReproduced":true,"explicitUntrustedPreventsAutoTrust":true,"globalConfigUnchanged":true,"modelCalls":0,"passed":true});
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-2/trust-write.json");
    fs::write(
        output,
        serde_json::to_string_pretty(&evidence).unwrap() + "\n",
    )
    .unwrap();
}

fn start(overlay: &std::path::Path, project: &std::path::Path, stderr: &std::path::Path) {
    let mut client = start_client(overlay, project, stderr);
    client.request("thread/start", json!({"cwd":project,"permissions":":danger-full-access","approvalPolicy":"never","ephemeral":true}));
}
