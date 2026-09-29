// Share the native protocol driver across provider and launcher journeys.
#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;
mod journeys;

#[test]
#[ignore = "live native Codex and gVisor; gate only"]
fn research_launcher_native_tools() {
    journeys::research();
}
