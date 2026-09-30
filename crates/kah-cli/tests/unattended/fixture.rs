use kah_core::run_store::RunStore;
use kah_run::store::{Record, Store};
use kah_sandbox::{Domain, Project};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub struct Fixture {
    pub root: PathBuf,
    pub log: PathBuf,
    pub id: String,
    pub child: Option<Child>,
}
impl Fixture {
    pub fn start(task: &str, options: &[&str]) -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!("kah-control-journey-{nonce}");
        let project =
            Project::select(&home.join("repos"), &home, Some((Domain::Mine, &name))).unwrap();
        project.initialize().unwrap();
        let log = home.join(format!(".cache/{name}.log"));
        let file = fs::File::create(&log).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_kah"))
            .current_dir(&project.root)
            .args([
                "run",
                "--model",
                "gpt-6-sol",
                "--effort",
                "low",
                "--time-limit",
                "300",
                "--test",
                "test -f marker.txt",
            ])
            .args(options)
            .arg(task)
            .stdout(Stdio::from(file.try_clone().unwrap()))
            .stderr(Stdio::from(file))
            .spawn()
            .unwrap();
        let mut f = Self {
            root: project.root,
            log,
            id: String::new(),
            child: Some(child),
        };
        f.await_id();
        f
    }
    fn await_id(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let text = fs::read_to_string(&self.log).unwrap();
            if let Some(id) = text.lines().find_map(|l| l.strip_prefix("Created ")) {
                self.id = id.into();
                return;
            }
            assert!(Instant::now() < deadline, "no run id: {text}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn state(&self) -> Record {
        Store::read(&Self::base(), &self.id).unwrap()
    }
    pub fn base() -> PathBuf {
        RunStore::default_state_root().unwrap().join("runs")
    }
    pub fn command(&self, command: &str, text: Option<&str>) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kah"));
        child.args([command, &self.id]);
        if let Some(text) = text {
            child.arg(text);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    pub fn wait_file(&mut self, name: &str) {
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if self.state().worktree.join(name).exists() {
                return;
            }
            assert!(
                self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                "worker ended early: {}",
                fs::read_to_string(&self.log).unwrap()
            );
            assert!(
                Instant::now() < deadline,
                "file {name} missing; {}",
                self.log.display()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn wait(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            if let Some(exit) = self.child.as_mut().unwrap().try_wait().unwrap() {
                return exit;
            }
            assert!(
                Instant::now() < deadline,
                "run timed out: {}",
                self.log.display()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn resume(&mut self) {
        let file = fs::OpenOptions::new().append(true).open(&self.log).unwrap();
        self.child = Some(
            Command::new(env!("CARGO_BIN_EXE_kah"))
                .args(["resume", &self.id])
                .current_dir(&self.root)
                .stdout(Stdio::from(file.try_clone().unwrap()))
                .stderr(Stdio::from(file))
                .spawn()
                .unwrap(),
        );
    }
    pub fn save(&self, name: &str, value: &serde_json::Value) {
        fs::write(
            std::env::var_os("KAH_GATE_EVIDENCE")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/gates/evidence/phase-4")
                })
                .join(format!("{name}.json")),
            serde_json::to_string_pretty(value).unwrap() + "\n",
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Keep failed fixtures for diagnosis. Successful gate cleanup is explicit.
    }
}
