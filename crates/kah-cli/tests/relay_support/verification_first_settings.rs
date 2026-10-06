//! Native execution records prove the fixture's tier/model/effort, not argv alone.
use super::*;

pub fn record(home: &Path, provider: &str, id: &str, courier: &Pane) -> Value {
    let endpoint = kah_cli::relay_store::pane_endpoint(
        &courier.id,
        &PathBuf::from(std::env::var_os("HERDR_SOCKET_PATH").unwrap()),
    )
    .unwrap();
    let mut records = vec![settings(
        home,
        "courier",
        &endpoint.project,
        &endpoint.session,
    )];
    if provider == "codex" {
        let run = Runs::read(&RunStore::default_state_root().unwrap().join("runs"), id).unwrap();
        let worker = run.native.unwrap().session.unwrap();
        let state = Reviews::read(&worker).unwrap().unwrap();
        records.push(settings(home, "worker", &state.worker.project, &worker));
        records.push(settings(
            home,
            "observer",
            &state.observer.workspace,
            state.observer.session.as_deref().unwrap(),
        ));
    }
    json!(records)
}
fn settings(home: &Path, role: &str, project: &Path, session: &str) -> Value {
    // Official installed source first: openai/codex rust-v0.160.0,
    // protocol/src/protocol.rs ThreadSettingsAppliedEvent/ThreadSettingsSnapshot;
    // app-server-protocol/src/protocol/v2/thread_data.rs Thread.path.
    let directory = Reviews::directory(session).unwrap();
    let mut history =
        kah_cli::native_history::History::open(home, &directory, "codex", project, session)
            .unwrap();
    let kah_cli::native_history::History::Codex { client, .. } = &mut history else {
        unreachable!()
    };
    let thread = client
        .request(
            "thread/read",
            json!({"threadId":session,"includeTurns":false}),
        )
        .unwrap();
    let path = PathBuf::from(thread["thread"]["path"].as_str().unwrap());
    let rows = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| {
            row["type"] == "event_msg" && row["payload"]["type"] == "thread_settings_applied"
        })
        .map(|row| row["payload"]["thread_settings"].clone())
        .collect::<Vec<_>>();
    assert!(!rows.is_empty(), "native settings absent for {role}");
    for row in &rows {
        assert_eq!(row["model"], "gpt-6.1-sol", "{role}");
        assert_eq!(row["reasoning_effort"], "low", "{role}");
        assert_eq!(
            row["service_tier"], "default",
            "fast must be OFF for {role}"
        );
    }
    json!({"role":role,"session":session,"path":path,"settings":rows})
}
