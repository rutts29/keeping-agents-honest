//! Native turn context proves model/effort; fixture invocation records prove requested tier.
use super::*;

pub fn record(
    home: &Path,
    provider: &str,
    id: &str,
    courier: &Pane,
    courier_args: &[String],
) -> Value {
    let endpoint = kah_cli::relay_store::pane_endpoint(
        &courier.id,
        &PathBuf::from(std::env::var_os("HERDR_SOCKET_PATH").unwrap()),
    )
    .unwrap();
    assert!(
        courier_args
            .iter()
            .any(|arg| arg == "service_tier=\"fast\"")
    );
    let mut records = vec![settings(
        home,
        "courier",
        &endpoint.project,
        &endpoint.session,
        json!({"source":"fixture native invocation argv","args":courier_args}),
    )];
    if provider == "codex" {
        let run = Runs::read(&RunStore::default_state_root().unwrap().join("runs"), id).unwrap();
        let native = run.native.unwrap();
        assert!(
            native
                .codex_config
                .iter()
                .any(|setting| setting == "service_tier=\"fast\"")
        );
        let tier = json!({"source":"kah persisted native invocation overrides; observer forwards the same service_tier via launch::run_settings","settings":native.codex_config,"executionTierClaim":false});
        let worker = native.session.unwrap();
        let state = Reviews::read(&worker).unwrap().unwrap();
        records.push(settings(
            home,
            "worker",
            &state.worker.project,
            &worker,
            tier.clone(),
        ));
        records.push(settings(
            home,
            "observer",
            &state.observer.workspace,
            state.observer.session.as_deref().unwrap(),
            tier,
        ));
    }
    json!(records)
}
fn settings(home: &Path, role: &str, project: &Path, session: &str, tier: Value) -> Value {
    // Official installed source first: openai/codex rust-v0.160.0,
    // core/session/mod.rs InitialHistory::New defers the initial turn context;
    // protocol/src/protocol.rs TurnContextItem carries model/effort, not tier.
    // Fresh sessions need not emit ThreadSettingsApplied.
    // The fixtures request native -c service_tier="fast".
    // config/read on this separate history client is NOT the TUI's runtime tier.
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
        .filter(|row| row["type"] == "turn_context")
        .map(|row| row["payload"].clone())
        .collect::<Vec<_>>();
    assert!(!rows.is_empty(), "native settings absent for {role}");
    for row in &rows {
        assert_eq!(row["model"], "gpt-6.1-sol", "{role}");
        assert_eq!(row["effort"], "low", "{role}");
    }
    json!({"role":role,"session":session,"path":path,"nativeTurnContexts":rows,
        "modelEffortSource":"native rollout turn_context",
        "requestedTier":{"value":"default","invocation":tier,"executionTierClaim":false}})
}
