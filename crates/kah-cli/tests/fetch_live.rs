use kah_run::{
    access, fetch_exchange::Broker, fetch_policy::FetchPolicy, fetch_runtime, launch,
    runtime::service_uid, verification,
};
use kah_sandbox::{Domain, current_uid, prepare_public_workspace, publish_binary};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[test]
#[ignore = "real gVisor and public HTTPS; no model call"]
fn allowed_fetch_works_with_raw_network_disabled() {
    let root = PathBuf::from(format!(
        "/var/tmp/kah-fetch-live-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let work = root.join("worktree");
    fs::create_dir(&work).unwrap();
    fs::write(work.join(".git"), "fixture\n").unwrap();
    let git = root.join("repo.git");
    fs::create_dir(&git).unwrap();
    let exchange = root.join("exchange");
    fs::create_dir(&exchange).unwrap();
    access::share_tree(&exchange, service_uid().unwrap(), true).unwrap();
    let policy = FetchPolicy::new("research", vec!["example.com".into()]).unwrap();
    let broker = Broker::start(exchange.clone(), policy).unwrap();
    let uid = current_uid().unwrap();
    let helper = publish_binary(
        Path::new(env!("CARGO_BIN_EXE_kah")),
        &PathBuf::from(format!("/var/tmp/kah-fetch-runtime-{uid}")),
    )
    .unwrap();
    let dummy = root.join("dummy");
    prepare_public_workspace(&dummy).unwrap();
    let image = fs::read_to_string("/usr/local/share/sbx/image-ref").unwrap();
    let executor = launch::sandbox(
        Domain::Research,
        &work,
        &git,
        &helper,
        image.trim(),
        service_uid().unwrap(),
        "run-fetch-probe",
    )
    .unwrap();
    let executor = fetch_runtime::mount(executor, &exchange, &helper).unwrap();
    let script = "set -e; /opt/kah __fetch https://example.com > /tmp/page; grep -q 'Example Domain' /tmp/page; if /opt/kah __fetch https://example.org >/tmp/no 2>/tmp/denied; then exit 7; fi; grep -q FETCH_NOT_ALLOWED /tmp/denied; if curl --connect-timeout 2 https://example.com >/tmp/direct 2>&1; then exit 8; fi; echo FETCH_BOUNDARY_OK";
    let output = verification::command(&executor, script, Duration::from_secs(40))
        .unwrap()
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("FETCH_BOUNDARY_OK"));
    drop(broker);
    fs::write(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/gates/evidence/phase-4/allowlist.json"),serde_json::to_string_pretty(&json!({"passed":true,"allowedHost":"example.com","otherHostRejected":true,"rawNetworkRefused":true,"rootChanges":false,"mechanism":"bounded read-only HTTPS via per-run file exchange"})).unwrap()+"\n").unwrap();
    fs::remove_dir_all(root).unwrap();
}
