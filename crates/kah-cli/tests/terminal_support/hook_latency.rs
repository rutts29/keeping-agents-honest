//! Controlled replay of a completed fixture tool, using the real native history
//! and live Jev. Both binaries are release builds; IDs distinguish replay events.
use kah_observer::session::{Binding, Store};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn measure(base: &Path, session: &str, claude: bool, transcript: &Path) -> Value {
    wait(base, session);
    let state = Store::read(base, session).unwrap().unwrap();
    let baseline = std::env::var("KAH_HOOK_BASELINE_BIN")
        .expect("KAH_HOOK_BASELINE_BIN names the executable to compare against");
    assert!(
        Path::new(&baseline).is_absolute() && Path::new(&baseline).is_file(),
        "KAH_HOOK_BASELINE_BIN must be an existing absolute executable"
    );
    let before = sample(
        base,
        &state.binding,
        claude,
        transcript,
        &baseline,
        "before",
    );
    let after = sample(
        base,
        &state.binding,
        claude,
        transcript,
        env!("CARGO_BIN_EXE_kah"),
        "after",
    );
    assert!(after["p95Ms"].as_f64().unwrap() < before["p95Ms"].as_f64().unwrap());
    assert!(after["p95Ms"].as_f64().unwrap() < 500.0);
    json!({"method":"12 sequential completed-tool hook replays per release binary, same real native user history and live Jev; replay IDs distinguish events. Measures whole hook process, not model/tool runtime.","before":before,"after":after})
}
fn sample(
    base: &Path,
    binding: &Binding,
    claude: bool,
    transcript: &Path,
    binary: &str,
    phase: &str,
) -> Value {
    let start_calls = Store::read(base, &binding.session)
        .unwrap()
        .unwrap()
        .usage
        .calls;
    let mut times = vec![];
    for n in 0..12 {
        let input = json!({"session_id":binding.session,"turn_id":format!("latency-{phase}"),"prompt_id":format!("latency-{phase}"),
            "cwd":binding.project,"transcript_path":transcript,"hook_event_name":"PostToolUse",
            "tool_use_id":format!("latency-{phase}-{n}"),"tool_name":if claude {"Bash"} else {"exec_command"},
            "tool_input":{"command":"printf 1 > tick-1.txt"},"tool_response":{"exit_code":0,"stdout":""}});
        let started = Instant::now();
        let output = invoke(binding, claude, binary, &input);
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(output.get("systemMessage").is_none(), "{output}");
    }
    wait(base, &binding.session);
    times.sort_by(f64::total_cmp);
    let calls = Store::read(base, &binding.session)
        .unwrap()
        .unwrap()
        .usage
        .calls
        - start_calls;
    json!({"samples":times,"p50Ms":times[5],"p95Ms":times[11],"jevCalls":calls})
}
fn invoke(binding: &Binding, claude: bool, binary: &str, input: &Value) -> Value {
    let mut command = Command::new(binary);
    command
        .arg(if claude { "__claude-hook" } else { "__hook" })
        .env(
            if claude {
                "KAH_CLAUDE_BINDING"
            } else {
                "KAH_OBSERVER_BINDING"
            },
            serde_json::to_string(binding).unwrap(),
        )
        .env_remove("CODEX_THREAD_ID")
        .env_remove("HERDR_ENV")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    if !claude {
        let base = kah_core::run_store::RunStore::default_state_root().unwrap();
        let policy = kah_cli::session_policy::saved(&base, &binding.session)
            .unwrap()
            .unwrap();
        command.env(
            "KAH_SESSION_POLICY",
            serde_json::to_string(&policy).unwrap(),
        );
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}
pub fn wait(base: &Path, session: &str) {
    let directory = Store::directory(base, session);
    let deadline = Instant::now() + Duration::from_secs(95);
    loop {
        let lock = fs::File::open(directory.join("tool-worker.lock"));
        if lock.is_err() || lock.unwrap().try_lock().is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "background hook worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
