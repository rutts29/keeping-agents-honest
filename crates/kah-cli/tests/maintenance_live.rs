use serde_json::json;
use std::{fs, os::unix::fs::symlink, path::PathBuf, process::Command};
const ID: &str = "11111111-1111-4111-8111-111111111111";
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "trace installed native deletion in disposable homes; no model or real credentials"]
fn inherited_home_identifies_the_index_replacement_writer() {
    let root = std::env::temp_dir().join(format!("kah-delete-writer-{}", std::process::id()));
    let f = Fixture(root);
    let global = f.0.join(".codex");
    let builder = f.0.join("builder");
    fs::create_dir_all(&global).unwrap();
    fs::create_dir(&builder).unwrap();
    fs::create_dir(global.join("sessions")).unwrap();
    symlink(global.join("sessions"), builder.join("sessions")).unwrap();
    for home in [&global, &builder] {
        fs::write(home.join("config.toml"), "").unwrap();
    }
    let row =
        json!({"id":ID,"thread_name":"synthetic fixture only","updated_at":"2026-10-02T00:00:00Z"})
            .to_string()
            + "\n";
    let native = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/bin/codex");
    let index = builder.join("session_index.jsonl");
    for clear_home in [false, true] {
        seed_rollout(&global);
        fs::write(global.join("session_index.jsonl"), &row).unwrap();
        symlink(global.join("session_index.jsonl"), &index).unwrap();
        let trace = f.0.join(format!("trace-{clear_home}"));
        let mut command = Command::new("strace");
        command
            .args(["-f", "-e", "trace=execve,rename,renameat,renameat2", "-o"])
            .arg(&trace)
            .arg(&native)
            .args(["delete", "--force", ID])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &f.0);
        if !clear_home {
            command.env("CODEX_HOME", &builder);
        }
        let result = command.output().unwrap();
        let trace = fs::read_to_string(trace).unwrap();
        assert!(
            trace.contains("execve(") && trace.contains("session_index.jsonl.tmp"),
            "native stderr: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            fs::symlink_metadata(&index)
                .unwrap()
                .file_type()
                .is_symlink(),
            clear_home
        );
        if !clear_home {
            assert_eq!(
                fs::read_to_string(global.join("session_index.jsonl")).unwrap(),
                row
            );
        }
        assert!(result.status.success());
        let evidence = json!({"passed":true,"nativeVersion":String::from_utf8(Command::new(&native).arg("--version").output().unwrap().stdout).unwrap().trim(),"nativeExit":result.status.code(),"inheritedHomeCleared":clear_home,"indexRemainedSymlink":clear_home,"writer":"native codex delete --force","renameSyscallObserved":true,"realLoginOrDaemonUsed":false,"modelCalls":0});
        save(clear_home, evidence);
        fs::remove_file(&index).unwrap();
    }
}

fn save(clear_home: bool, evidence: serde_json::Value) {
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join(format!("index-writer-{clear_home}.json")),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

fn seed_rollout(home: &std::path::Path) {
    let directory = home.join("sessions/2026/10/02");
    fs::create_dir_all(&directory).unwrap();
    let rows = [
        json!({"timestamp":"2026-10-02T00:00:00Z","type":"session_meta","payload":{"session_id":ID,"id":ID,"timestamp":"2026-10-02T00:00:00Z","cwd":home,"originator":"fixture","cli_version":"0.160.0","source":"cli","model_provider":"openai"}}),
        json!({"timestamp":"2026-10-02T00:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"synthetic deletion fixture","images":[],"local_images":[]}}),
    ];
    fs::write(
        directory.join(format!("rollout-2026-10-02T00-00-00-{ID}.jsonl")),
        rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
    )
    .unwrap();
}
