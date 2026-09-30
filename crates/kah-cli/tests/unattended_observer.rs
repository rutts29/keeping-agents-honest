use kah_cli::unattended_observer::WorkerReply;
use serde_json::json;
#[test]
fn worker_claims_remain_reports_and_external_requests_need_a_question() {
    let reply = |status: &str, question: &str| {
        vec![
            json!({"kind":"agentMessage","evidence":{"text":json!({"status":status,"summary":"claimed done","question":question}).to_string()}}),
        ]
    };
    assert_eq!(
        WorkerReply::from_actions(&reply("ready", ""))
            .unwrap()
            .status,
        "ready"
    );
    assert!(WorkerReply::from_actions(&reply("complete", "")).is_err());
    assert!(WorkerReply::from_actions(&reply("external_action_required", "")).is_err());
    assert_eq!(
        WorkerReply::from_actions(&reply(
            "external_action_required",
            "Please publish after review"
        ))
        .unwrap()
        .question,
        "Please publish after review"
    );
}
