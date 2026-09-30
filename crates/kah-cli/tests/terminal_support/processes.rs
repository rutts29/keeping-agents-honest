//! Reap only the test PTY's groups, including detached descendants carrying its tag.
use std::{collections::BTreeSet, fs, process::Command, thread, time::Duration};
#[derive(Clone)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub group: u32,
    pub state: char,
    pub cwd: String,
}
pub fn list() -> Vec<Process> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return vec![];
    };
    entries
        .flatten()
        .filter_map(|e| {
            let pid = e.file_name().to_str()?.parse().ok()?;
            let stat = fs::read_to_string(e.path().join("stat")).ok()?;
            let fields = stat
                .rsplit_once(") ")?
                .1
                .split_whitespace()
                .collect::<Vec<_>>();
            Some(Process {
                pid,
                parent: fields.get(1)?.parse().ok()?,
                group: fields.get(2)?.parse().ok()?,
                state: fields.first()?.chars().next()?,
                cwd: fs::read_link(e.path().join("cwd"))
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            })
        })
        .collect()
}
fn tagged(pid: u32, tag: &str) -> bool {
    fs::read(format!("/proc/{pid}/environ")).is_ok_and(|b| {
        b.split(|v| *v == 0)
            .any(|v| v == format!("KAH_LIVE_FIXTURE={tag}").as_bytes())
    })
}
pub fn reap(root: u32, tag: &str) {
    let processes = list();
    let mut descendants = BTreeSet::new();
    if tagged(root, tag) {
        descendants.insert(root);
    }
    loop {
        let before = descendants.len();
        for p in &processes {
            if descendants.contains(&p.parent) || tagged(p.pid, tag) {
                descendants.insert(p.pid);
            }
        }
        if before == descendants.len() {
            break;
        }
    }
    let self_group = processes
        .iter()
        .find(|p| p.pid == std::process::id())
        .map(|p| p.group);
    let groups = processes
        .iter()
        .filter(|p| descendants.contains(&p.pid) && p.group > 1 && Some(p.group) != self_group)
        .map(|p| p.group)
        .collect::<BTreeSet<_>>();
    for signal in ["-TERM", "-KILL"] {
        for group in &groups {
            let _ = Command::new("/usr/bin/kill")
                .args([signal, "--", &format!("-{group}")])
                .output();
        }
        thread::sleep(Duration::from_millis(150));
    }
}
pub fn leftovers() -> Vec<Process> {
    list()
        .into_iter()
        .filter(|p| p.state != 'Z' && p.pid != std::process::id())
        .filter(|p| {
            p.cwd.split('/').any(|s| {
                s.starts_with("kah-gate")
                    || s.starts_with("kah-claude-")
                    || s.starts_with("kah-phase1-")
                    || s.starts_with("kah-fixture-")
            }) || fs::read(format!("/proc/{}/environ", p.pid)).is_ok_and(|b| {
                b.split(|v| *v == 0)
                    .any(|v| v.starts_with(b"KAH_LIVE_FIXTURE="))
            })
        })
        .collect()
}

pub fn fixture_containers() -> Result<Vec<String>, String> {
    let uid = kah_run::runtime::service_uid()?;
    let output = Command::new("/usr/bin/docker")
        .args([
            "--host",
            &format!("unix:///run/user/{uid}/docker.sock"),
            "ps",
            "--format",
            "{{.ID}} {{.Labels}}",
            "--filter",
            "label=dev.kah.project",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("could not audit rootless containers".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| {
            l.contains("/kah-gate") || l.contains("/kah-phase1-") || l.contains("/kah-fixture-")
        })
        .filter_map(|l| l.split_whitespace().next().map(str::to_owned))
        .collect())
}
