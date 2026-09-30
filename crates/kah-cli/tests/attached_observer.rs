#[path = "journeys/fixture.rs"]
mod fixture;
use kah_codex::{control::Control, source};
use serde_json::json;
use std::{path::PathBuf, time::Duration};

#[test]
#[ignore = "native context injection and source-role proof; no model call"]
fn injected_observer_context_is_never_an_authoritative_user_record() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (launch, mut fixture) = fixture::prepare(&home, "mine", false);
    let mut client = Control::connect(
        &home.join(".local/bin/codex"),
        &launch.overlay,
        &fixture.project,
        Duration::from_secs(20),
    )
    .unwrap();
    let start = client
        .request("thread/start", json!({"cwd":fixture.project}))
        .unwrap();
    let session = start["thread"]["id"].as_str().unwrap();
    fixture.sessions.push(session.into());
    kah_cli::attached_observer::inject(
        &mut client,
        session,
        "kah observation fixture; not user steering",
    )
    .unwrap();
    let snapshot = source::observe(&mut client, &fixture.project, session).unwrap();
    assert!(
        snapshot.users.is_none(),
        "developer observation became user authority"
    );
}
