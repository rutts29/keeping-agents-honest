use kah_cli::{Arguments, Launch};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};
pub(super) struct Fixture {
    pub(super) project: PathBuf,
    pub(super) canary: PathBuf,
    pub(super) overlay: PathBuf,
    pub(super) stderr: PathBuf,
    pub(super) sessions: Vec<String>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for id in &self.sessions {
            let _ = Command::new("/home/example/.local/bin/codex")
                .args(["delete", "--force", id])
                .env_remove("HERDR_ENV")
                .env_remove("CODEX_THREAD_ID")
                .output();
        }
        let ids = Command::new("/usr/bin/docker")
            .args([
                "--host",
                "unix:///run/user/2001/docker.sock",
                "ps",
                "-aq",
                "--filter",
            ])
            .arg(format!("label=dev.kah.project={}", self.project.display()))
            .output()
            .unwrap();
        for id in String::from_utf8_lossy(&ids.stdout).split_whitespace() {
            let _ = Command::new("/usr/bin/docker")
                .args([
                    "--host",
                    "unix:///run/user/2001/docker.sock",
                    "rm",
                    "-f",
                    id,
                ])
                .output();
        }
        let _ = fs::remove_dir_all(&self.project);
        let _ = fs::remove_dir_all(&self.overlay);
        let _ = fs::remove_file(&self.canary);
        let _ = fs::remove_file(&self.stderr);
    }
}

pub(super) fn prepare(home: &Path, domain: &str, online: bool) -> (Launch, Fixture) {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("kah-gate2-{nonce}");
    let mut input = vec![
        domain.into(),
        name.clone(),
        "--model".into(),
        "gpt-6-sol".into(),
        "--effort".into(),
        "low".into(),
    ];
    if online {
        input.push("--online".into());
    }
    let args = Arguments::parse(input).unwrap();
    let launch = Launch::prepare(home, home, args).unwrap();
    let f = Fixture {
        project: launch.project.root.clone(),
        overlay: launch.overlay.clone(),
        canary: home.join(format!(".cache/{name}-canary")),
        stderr: home.join(format!(".cache/{name}-stderr")),
        sessions: vec![],
    };
    fs::write(&f.canary, "untouched").unwrap();
    (launch, f)
}
