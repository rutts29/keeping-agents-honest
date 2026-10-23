//! Retire detached kah jobs before deleting the exact fixture project they read.
use std::{fs, path::Path, process::Command, thread, time::Duration};

pub fn cleanup(project: &Path, sessions: &[String]) {
    let Ok(entries) = fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(cwd) = fs::read_link(entry.path().join("cwd")) else {
            continue;
        };
        let Ok(command) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        if !owned(project, sessions, &cwd, &command) {
            continue;
        }
        // hook::spawn_command gives each detached job its own process group.
        if group(pid) != Some(pid) {
            continue;
        }
        let members = members(pid);
        for signal in ["-TERM", "-KILL"] {
            if !members.iter().any(|(member, stamp)| {
                kah_observer::session::process_stamp(*member).as_ref() == Some(stamp)
            }) {
                break;
            }
            let _ = Command::new("/usr/bin/kill")
                .args([signal, "--", &format!("-{pid}")])
                .output();
            // A shutdown handshake, not a review deadline or a content cap.
            thread::sleep(Duration::from_millis(150));
        }
    }
}

fn group(pid: u32) -> Option<u32> {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()?
        .rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(2)?
        .parse()
        .ok()
}

fn members(group_id: u32) -> Vec<(u32, String)> {
    fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse().ok()?;
            (group(pid) == Some(group_id))
                .then(|| kah_observer::session::process_stamp(pid).map(|stamp| (pid, stamp)))
                .flatten()
        })
        .collect()
}

fn owned(project: &Path, sessions: &[String], cwd: &Path, command: &[u8]) -> bool {
    let path = project.to_string_lossy();
    let args = command.split(|b| *b == 0).collect::<Vec<_>>();
    (cwd == project || cwd.to_string_lossy() == format!("{path} (deleted)"))
        && args
            .first()
            .is_some_and(|p| p.rsplit(|b| *b == b'/').next() == Some(b"kah".as_slice()))
        && args.get(1).is_some_and(|arg| arg.starts_with(b"__"))
        && args
            .get(2)
            .is_some_and(|id| sessions.iter().any(|s| s.as_bytes() == *id))
}

#[test]
fn only_exact_fixture_cwd_and_registered_worker_jobs_are_owned() {
    let project = Path::new("/home/fixture/repos/research/kah-gate2-owned");
    let sessions = vec!["worker".into()];
    let command = b"/home/fixture/.local/bin/kah\0__checkpoint-tags\0worker\0";
    assert!(owned(project, &sessions, project, command));
    assert!(owned(
        project,
        &sessions,
        Path::new("/home/fixture/repos/research/kah-gate2-owned (deleted)"),
        command
    ));
    assert!(!owned(
        project,
        &sessions,
        Path::new("/home/fixture/repos/mine/kah"),
        command
    ));
    assert!(!owned(
        project,
        &["another-worker".into()],
        project,
        command
    ));
    assert!(!owned(
        project,
        &sessions,
        project,
        b"/bin/bash\0__checkpoint-tags\0worker\0"
    ));
}
