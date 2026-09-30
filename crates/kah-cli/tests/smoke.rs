use kah_cli::smoke::command_passed;
use serde_json::json;
#[test]
fn smoke_requires_a_real_successful_native_command_not_an_agent_claim() {
    let message =
        json!({"type":"item.completed","item":{"type":"agent_message","text":"KAH_SMOKE_OK"}})
            .to_string();
    assert!(!command_passed(message.as_bytes(), "/host/canary"));
    let command = |code| {
        json!({"type":"item.completed","item":{"type":"command_execution","command":"test ! -e /host/canary && test -d .git && echo KAH_SMOKE_OK","exit_code":code,"aggregated_output":"KAH_SMOKE_OK\n"}}).to_string()
    };
    assert!(!command_passed(command(1).as_bytes(), "/host/canary"));
    assert!(command_passed(command(0).as_bytes(), "/host/canary"));
}

#[test]
fn smoke_rejects_an_unrelated_command_with_the_success_marker() {
    let forged=json!({"type":"item.completed","item":{"type":"command_execution","command":"echo KAH_SMOKE_OK","exit_code":0,"aggregated_output":"KAH_SMOKE_OK\n"}}).to_string();
    assert!(!command_passed(forged.as_bytes(), "/host/canary"));
}
