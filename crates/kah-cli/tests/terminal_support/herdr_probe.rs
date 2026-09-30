//! An isolated, fixture-owned Herdr session shared by provider journeys.
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    env, fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime},
};
pub struct Probe {
    pub root: PathBuf,
    pub name: String,
}
impl Drop for Probe {
    fn drop(&mut self) {
        // Only the unpredictable named session this fixture created is touched.
        for action in ["stop", "delete"] {
            let _ = Command::new("herdr")
                .args(["session", action, &self.name, "--json"])
                .output();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn setup() -> Probe {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("kah-gate-{nonce}");
    let root = env::temp_dir().join(&name);
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let shell = root.join("shell");
    fs::write(
        &shell,
        "#!/bin/sh\nexec env PS1=KAH_HERDR_SHELL_READY /bin/bash --noprofile --norc -i\n",
    )
    .unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        root.join("config.toml"),
        format!(
            "onboarding=false\n[terminal]\ndefault_shell={}\n",
            serde_json::to_string(&shell).unwrap()
        ),
    )
    .unwrap();
    Probe { root, name }
}

pub fn wait_capture(probe: &Probe) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(data) = fs::read(probe.root.join("capture.json"))
            && let Ok(value) = serde_json::from_slice::<Value>(&data)
            && value["complete"] == true
        {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "Herdr capture did not finish: {}",
            fs::read_to_string(probe.root.join("helper.log")).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(200));
    }
}

pub fn capture() {
    assert_eq!(env::var("HERDR_ENV").unwrap(), "1");
    let pane = env::var("HERDR_PANE_ID").unwrap();
    let output = PathBuf::from(env::var_os("KAH_HERDR_CAPTURE").unwrap());
    let marker = PathBuf::from(env::var_os("KAH_HERDR_MARKER").unwrap());
    let mut states = BTreeSet::new();
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let result = Command::new("herdr")
            .args(["pane", "get", &pane])
            .output()
            .unwrap();
        assert!(result.status.success());
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        let pane = &value["result"]["pane"];
        let status = pane["agent_status"].as_str().unwrap_or("unknown");
        states.insert(status.to_owned());
        let complete = marker.exists()
            && states.contains("working")
            && matches!(status, "idle" | "done")
            && pane.get("agent_session").is_some();
        fs::write(
            &output,
            serde_json::to_vec(&json!({"pane":pane,"states":states,"complete":complete})).unwrap(),
        )
        .unwrap();
        if complete {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Herdr never observed the completed native session"
        );
        thread::sleep(Duration::from_millis(200));
    }
}
