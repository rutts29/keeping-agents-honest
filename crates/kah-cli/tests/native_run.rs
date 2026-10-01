use kah_cli::native_run::shell_command;
use std::process::Command;

#[test]
fn native_task_and_environment_are_shell_data_not_commands() {
    let mut native = Command::new("/usr/bin/printf");
    native
        .env_remove("KAH_UNUSED")
        .env("KAH_VALUE", "$(false) ' literal");
    native.args(["%s", "hello ' $(false) `false`\nsecond line"]);
    let shell = shell_command(&native).unwrap();
    let output = Command::new("/bin/sh")
        .args(["-c", &shell])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "hello ' $(false) `false`\nsecond line"
    );
}

#[test]
fn herdr_mutations_accept_native_non_json_success() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let root = std::env::temp_dir().join(format!("kah-herdr-protocol-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let shim = root.join("herdr");
    fs::write(&shim, "#!/bin/sh\ncase \"$1 $2\" in\n'pane split') echo '{\"result\":{\"pane\":{\"pane_id\":\"fixture-pane\"}}}';;\n'pane get') echo '{\"result\":{\"pane\":{\"agent\":null,\"foreground_cwd\":\"/tmp\"}}}';;\n*) echo submitted;;\nesac\n").unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o700)).unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "herdr_protocol_child"])
        .env("KAH_HERDR_PROTOCOL_FIXTURE", &root)
        .env("HERDR_ENV", "1")
        .env("PATH", &root)
        .output()
        .unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
}

#[test]
fn herdr_protocol_child() {
    let Some(root) = std::env::var_os("KAH_HERDR_PROTOCOL_FIXTURE") else {
        return;
    };
    let mut command = Command::new("/bin/true");
    command.current_dir(root);
    let pane = kah_cli::run_pane::Pane::start(&command).unwrap();
    assert_eq!(pane.id, "fixture-pane");
    pane.prompt("fixture text").unwrap();
    pane.close().unwrap();
}
