use super::{Fixture, pty};
use kah_observer::session::{State, Store};
use serde_json::Value;
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};

pub fn version() -> String {
    let output = Command::new("claude").arg("--version").output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}
pub fn state(f: &Fixture) -> State {
    let state = Store::read(&f.base, &f.id).unwrap().unwrap();
    assert!(state.gap.is_none(), "{:?}", state.gap);
    assert!(state.judgments.iter().all(|j| j.backend == "jev"));
    state
}
pub fn finish(terminal: &mut pty::Terminal) {
    terminal.submit("/exit");
    terminal.wait_exit(false);
}
pub fn write(f: &Fixture, name: &str, mut value: Value) {
    value["observer"] = super::claude_support::observer::evidence();
    fs::write(
        f.evidence.join(name),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}
pub fn native_usage(f: &Fixture) -> Value {
    f.rows()
        .into_iter()
        .rfind(|r| r["type"] == "cost-state")
        .map(|r| r["modelUsage"].clone())
        .unwrap_or(Value::Null)
}
/// Kills the in-flight observer hook and returns its pid with the session
/// store lock. While the caller holds the lock, no later observation can
/// recover and then clear the gap before `kah status` reports it.
pub fn kill_observer(f: &Fixture) -> (u32, Store) {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(state) = Store::read(&f.base, &f.id).unwrap()
            && let Some(flight) = state.inflight
        {
            let root = format!("/proc/{}", flight.pid);
            let cmd = fs::read(format!("{root}/cmdline")).unwrap_or_default();
            let stat = fs::read_to_string(format!("{root}/stat")).unwrap_or_default();
            let stamp = stat
                .rsplit_once(") ")
                .and_then(|(_, s)| s.split_whitespace().nth(19));
            if cmd.split(|b| *b == 0).any(|s| s == b"__claude-hook")
                && fs::read_link(format!("{root}/cwd")).ok() == Some(state.binding.project)
                && stamp == Some(flight.process_stamp.as_str())
            {
                assert!(
                    Command::new("/bin/kill")
                        .args(["-KILL", &flight.pid.to_string()])
                        .status()
                        .unwrap()
                        .success()
                );
                // SIGKILL releases the killed hook's lock; take it at once.
                let held = Store::open(&f.base, &f.id, Duration::from_secs(5)).unwrap();
                return (flight.pid, held);
            }
        }
        assert!(
            Instant::now() < deadline,
            "fixture observer never became active"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
pub fn gap_status(f: &Fixture) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = Command::new(env!("CARGO_BIN_EXE_kah"))
            .args(["status", &f.id, "--json"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let status: Value = serde_json::from_slice(&output.stdout).unwrap();
        if status[0]["gap"]["code"] == "OBSERVER_INTERRUPTED" {
            return status[0]["gap"].clone();
        }
        assert!(
            Instant::now() < deadline,
            "interruption gap not visible in status"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
