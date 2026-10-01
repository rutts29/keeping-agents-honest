use kah_sandbox::{Domain, Executor, LaunchOptions, Project, publish_binary};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture {
    root: PathBuf,
    socket: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let label = format!("label=dev.kah.project={}", self.root.join("work").display());
        if let Ok(ids) = Command::new("docker")
            .args(["--host", &self.socket, "ps", "-aq", "--filter", &label])
            .output()
        {
            for id in String::from_utf8_lossy(&ids.stdout).split_whitespace() {
                let _ = Command::new("docker")
                    .args(["--host", &self.socket, "rm", "-f", id])
                    .output();
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
#[ignore = "gVisor native Claude boot and no-credential inference diagnostic; no model account is supplied"]
fn claude_in_gvisor_without_credentials() {
    let (f, executor, native) = setup();
    let version = run(&executor, &["/opt/codex", "--version"]);
    let boot = version.status.success();
    let isolation = run(
        &executor,
        &[
            "/bin/sh",
            "-c",
            &format!(
                "test \"$(id -u)\" = 65534 && test ! -e '{}' && test ! -e /run/user/2001/docker.sock && echo ISOLATED",
                f.root.join("host-canary").display()
            ),
        ],
    );
    assert!(isolation.status.success());
    let print = run(
        &executor,
        &[
            "/opt/codex",
            "-p",
            "--tools",
            "",
            "--no-session-persistence",
            "--output-format",
            "json",
            "Reply KAH_SANDBOX_PROBE.",
        ],
    );
    let text = format!(
        "{} {}",
        String::from_utf8_lossy(&print.stdout),
        String::from_utf8_lossy(&print.stderr)
    );
    let auth_missing = text.to_ascii_lowercase().contains("not logged in")
        || text.contains("API key")
        || text.contains("/login");
    let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(evidence.join("claude-runtime.json"), serde_json::to_vec_pretty(&json!({
        "installedBinary":native,"gvisorVersion":String::from_utf8_lossy(&version.stdout).trim(),"bootPassed":boot,
        "gvisorRuntime":"runsc","network":"none","user":65534,"hostCanaryHidden":true,"hostDockerSocketHidden":true,
        "credentialsMounted":false,"printExit":print.status.code(),"inferencePassed":false,"authenticationMissing":auth_missing,
        "unattendedJourneyPassed":false,"decision":"credential-free inference boundary still required"})).unwrap()).unwrap();
    assert!(
        boot,
        "Claude boot: {}",
        String::from_utf8_lossy(&version.stderr)
    );
    assert!(
        !print.status.success() && auth_missing,
        "unexpected credential-free probe result: {}",
        text.chars().take(1500).collect::<String>()
    );
}
fn run(executor: &Executor, command: &[&str]) -> Output {
    Command::new("timeout")
        .args(["--kill-after=2s", "45s", "docker"])
        .args(&executor.args)
        .args(command)
        .output()
        .unwrap()
}

fn setup() -> (Fixture, Executor, PathBuf) {
    let uid = kah_run::access::service_uid().unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(format!("/var/tmp/kah-claude-runtime-{nonce}"));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let f = Fixture {
        root,
        socket: format!("unix:///run/user/{uid}/docker.sock"),
    };
    fs::create_dir(f.root.join("work")).unwrap();
    fs::write(f.root.join("host-canary"), "not a credential").unwrap();
    kah_run::access::traverse(&f.root, uid).unwrap();
    kah_run::access::share_tree(&f.root.join("work"), uid, true).unwrap();
    let native = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join(".local/bin/claude")
        .canonicalize()
        .unwrap();
    let public = publish_binary(&native, &f.root.join("runtime")).unwrap();
    let image = fs::read_to_string("/usr/local/share/sbx/image-ref").unwrap();
    let project = Project {
        domain: Domain::Mine,
        root: f.root.join("work"),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let mut executor = Executor::new(
        &project,
        LaunchOptions::default(),
        &public,
        image.trim(),
        uid,
    )
    .unwrap();
    for arg in &mut executor.args {
        if arg == "0:0" {
            *arg = "65534:65534".into();
        } else if arg == "/home/sandbox:rw,nosuid,nodev,size=512m,mode=700" {
            arg.push_str(",uid=65534,gid=65534");
        }
    }
    executor.insert_options(["--pull".into(), "never".into()]);
    executor.args.truncate(executor.args.len() - 4);
    (f, executor, native)
}
