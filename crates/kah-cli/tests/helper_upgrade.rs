//! Real host-process re-exec, not an agent or a provider simulation.
use std::{fs, path::Path, process::Command, time::Duration};

#[test]
fn installed_helper_reexecs_with_the_same_pid_identity_and_singleton_lock() {
    run(false);
}

#[test]
fn spawned_watchers_keep_their_inherited_stdin_lock_across_reexec() {
    run(true);
}

fn run(inherited: bool) {
    let root = Temp(std::env::temp_dir().join(format!(
        "kah-helper-upgrade-{}-{inherited}-{}",
        std::process::id(),
        kah_observer::session::now_ms()
    )));
    let home = root.0.as_path();
    kah_core::run_store::ensure_private_directory(home).unwrap();
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/helper_upgrade.rs");
    let fixture = home.join("daemon.rs");
    fs::write(&fixture, daemon_source(&source)).unwrap();
    compile(&fixture, &home.join("old"), true);
    compile(&fixture, &home.join("new"), false);
    let installed = home.join(".local/bin/kah");
    fs::copy(home.join("old"), &installed).unwrap();
    let mut child = start(&installed, home, inherited);
    let pid = child.0.id();
    wait(home, "ready");
    fs::write(
        home.join("record-stage"),
        r#"{"encoding":"json_text","payload":"new-format"}"#,
    )
    .unwrap();
    fs::rename(home.join("record-stage"), home.join("record")).unwrap();
    wait(home, "old-error");
    fs::copy(home.join("new"), home.join("replacement")).unwrap();
    fs::rename(home.join("replacement"), &installed).unwrap();
    let read = wait(home, "read");
    let lease = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(home.join("singleton.lock"))
        .unwrap();
    assert!(lease.try_lock().is_err(), "singleton ownership was lost");
    fs::write(home.join("stop"), "").unwrap();
    assert!(child.0.wait().unwrap().success());
    assert_eq!(read, format!("{pid}:same-worker-identity:new-format"));
}

fn start(installed: &Path, home: &Path, inherited: bool) -> OwnedChild {
    let mut command = Command::new(installed);
    command.args([
        "__review-liveness-watch",
        "same-worker-identity",
        if inherited { "stdin" } else { "lease" },
    ]);
    command.env("HOME", home);
    if inherited {
        let lease = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(home.join("singleton.lock"))
            .unwrap();
        lease.try_lock().unwrap();
        command.stdin(std::process::Stdio::from(lease));
    }
    // The parallel test may fork while our just-copied binary is still open
    // for writing in its child; retry only that ETXTBSY, at most 3 tries.
    for attempt in 1.. {
        match command.spawn() {
            Ok(child) => return OwnedChild(child),
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && attempt < 3 => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => panic!("{e}"),
        }
    }
    unreachable!()
}

fn compile(source: &Path, output: &Path, legacy: bool) {
    let mut command = Command::new("rustc");
    command.args(["--edition=2024", "--check-cfg=cfg(legacy)"]);
    if legacy {
        command.args(["--cfg", "legacy"]);
    }
    let status = command.arg(source).arg("-o").arg(output).status().unwrap();
    assert!(status.success());
}

fn wait(root: &Path, name: &str) -> String {
    let start = std::time::Instant::now();
    loop {
        if let Ok(text) = fs::read_to_string(root.join(name))
            && !text.is_empty()
        {
            return text;
        }
        assert!(start.elapsed() < Duration::from_secs(3), "missing {name}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct OwnedChild(std::process::Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn daemon_source(source: &Path) -> String {
    format!(
        r##"type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[path = {source:?}] mod helper_upgrade;
fn main() -> Result<()> {{
    let root = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let lease = helper_upgrade::open_lease(&root.join("singleton.lock"))?;
    lease.try_lock()?;
    std::fs::write(root.join("ready"), "ready")?;
    loop {{
        helper_upgrade::between_samples(if std::env::args().nth(3).as_deref() == Some("stdin") {{ None }} else {{ Some(&lease) }})?;
        if root.join("stop").exists() {{ return Ok(()); }}
        if let Ok(record) = std::fs::read_to_string(root.join("record")) {{
            if cfg!(legacy) && record.contains("json_text") {{
                std::fs::write(root.join("old-error"), "REVIEW_SOURCE_ENCODING")?;
            }} else {{
                assert_eq!(record, r#"{{"encoding":"json_text","payload":"new-format"}}"#);
                let identity = std::env::args().nth(2).unwrap();
                std::fs::write(root.join("read"), format!("{{}}:{{identity}}:new-format", std::process::id()))?;
            }}
        }}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }}
}}
"##,
        source = source
    )
}
