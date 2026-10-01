use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

pub const BASH_CREATE: &str = "mkdir ownership; printf 'ALPHA\\n' > ownership/from-bash.txt; git add -- ownership/from-bash.txt; git commit --no-gpg-sign -m 'test: sandbox creates file'";
pub const BASH_FINISH: &str = "printf 'BASH_APPEND\\n' >> ownership/from-native.txt; git add -- ownership; git commit --no-gpg-sign -m 'test: sandbox commits native edits'";

pub fn prepare(project: &Path) {
    for (key, value) in [
        ("user.name", "kah fixture"),
        ("user.email", "kah@localhost"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(project)
                .args(["config", key, value])
                .status()
                .unwrap()
                .success()
        );
    }
}

pub fn verify_host(project: &Path) -> Value {
    assert_eq!(
        fs::read_to_string(project.join("ownership/from-bash.txt"))
            .unwrap()
            .trim(),
        "BETA"
    );
    let native = fs::read_to_string(project.join("ownership/from-native.txt")).unwrap();
    assert!(native.contains("NATIVE") && native.contains("BASH_APPEND"));
    let script = "set -eu\nprintf '\\nHOST_APPEND\\n' >> ownership/from-bash.txt\nprintf '\\nHOST_APPEND\\n' >> ownership/from-native.txt\nprintf 'HOST_NEW\\n' > ownership/from-host.txt\ngit add -- ownership\ngit commit --no-gpg-sign -m 'test: host edits sandbox files'\ntest -z \"$(git status --porcelain -- ownership)\"\nrm -r -- ownership\ngit add -u -- ownership\ngit commit --no-gpg-sign -m 'test: host removes sandbox files'\ntest -z \"$(git status --porcelain -- ownership)\"";
    let result = Command::new("/bin/bash")
        .args(["--noprofile", "--norc", "-c", script])
        .current_dir(project)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "plain-host ownership check: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    // Removing fixture Git metadata as the ordinary user also checks every
    // sandbox-created object/ref directory, including intentionally read-only objects.
    fs::remove_dir_all(project.join(".git")).unwrap();
    json!({"nativeEditedSandboxFile":true,"sandboxEditedNativeFile":true,"sandboxGitCommits":true,
        "hostAppendAndNewFile":true,"hostGitCommitAndStatus":true,"hostDeletedSandboxTreeAndGitMetadata":true,
        "ownershipRewriting":false,"hostUid":kah_sandbox::current_uid().unwrap()})
}
