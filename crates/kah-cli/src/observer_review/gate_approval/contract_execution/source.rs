//! V12 executes the explicit thinker plan; older protocols retain their rule.
use super::*;

pub(super) fn validate(request: &Request, coverage: &Value, command: &str) -> Result<()> {
    // execute_reasoned_check is set by proposed_target(), only for v12 plans.
    // This authorizes a whole planned check, never extraction from worker shell.
    if request.trigger["interactive_gate_verification"].is_null()
        || coverage["execute_reasoned_check"] == true
    {
        return Ok(());
    }
    let proof =
        super::super::captured(request)?.ok_or("REVIEW_GATE_CHECK_SOURCE stage=verification")?;
    if !proof.tests.iter().any(|t| t.command == command) {
        return Err("REVIEW_GATE_CHECK_SOURCE stage=verification: legacy interactive gate requires a recorded project check".into());
    }
    Ok(())
}
