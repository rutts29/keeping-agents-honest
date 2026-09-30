use kah_run::{
    fetch_exchange::{Broker, read_regular, request},
    fetch_policy::FetchPolicy,
};
use std::{fs, os::unix::fs::symlink};
#[test]
fn broker_rejects_unlisted_hosts_and_never_reads_symlinked_request_files() {
    let root = std::env::temp_dir().join(format!("kah-fetch-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("private"), "not request data").unwrap();
    symlink(root.join("private"), root.join("request-aa.json")).unwrap();
    assert!(read_regular(&root.join("request-aa.json"), 8192).is_err());
    let broker = Broker::start(
        root.clone(),
        FetchPolicy::new("research", vec!["example.com".into()]).unwrap(),
    )
    .unwrap();
    assert!(
        request(&root, "https://not-allowed.test/")
            .unwrap_err()
            .starts_with("FETCH_NOT_ALLOWED")
    );
    drop(broker);
    fs::remove_dir_all(root).unwrap();
}
