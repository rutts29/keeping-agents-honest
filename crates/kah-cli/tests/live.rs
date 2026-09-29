// Share the native protocol driver across provider and launcher journeys.
#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;
mod journeys;

#[test]
#[ignore = "live native Codex and gVisor; gate only"]
fn research_launcher_native_tools() {
    journeys::research();
}

#[test]
#[ignore = "live native tools, hosted web search and gVisor; gate only"]
fn tasks_and_search_policy_in_all_domains() {
    journeys::all_domains();
}

#[test]
#[ignore = "live native model switch, process resume and executor loss; gate only"]
fn model_resume_and_executor_loss() {
    journeys::resume_and_loss();
}
