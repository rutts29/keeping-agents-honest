use super::{Report, Request, Value};
use crate::Result;
use kah_observer::{decision_units::ReviewUnits, judge::Judgment};

pub fn context(request: &Request, report: &Report, input: &mut Value) -> Result<()> {
    if request.purpose != crate::observer_review::state::Purpose::Steering {
        return Ok(());
    }
    let source: kah_observer::intake::Input =
        serde_json::from_value(request.trigger["uncertain_user_input"].clone())?;
    super::super::steering::verify_proposal(request, report)?;
    input["event"]["user_text"] = source.text.into();
    input["prior_intent"] = request.trigger["steering_context"]["prior_intent"].clone();
    input["newer"] = request.trigger["steering_context"]["newer"].clone();
    Ok(())
}
pub fn apply(
    request: &Request,
    units: &ReviewUnits,
    judgment: &Judgment,
    assessment: &mut Value,
) -> Result<()> {
    if request.purpose != crate::observer_review::state::Purpose::Steering
        || assessment["passed"] != true
    {
        return Ok(());
    }
    let application = super::super::steering::apply_proposal(
        request,
        units
            .steering
            .as_ref()
            .ok_or("REVIEW_STEERING_PROPOSAL stage=decision")?,
        judgment,
    )?;
    assessment["application"] = application.into();
    assessment["steering_directs"] = units.steering.as_ref().is_some_and(|p| p.directs).into();
    if application == "intent_changed_recheck_next_turn" {
        assessment["state"] = "steering_needs_thinker".into();
        assessment["passed"] = Value::Null;
        assessment["needs_thinker"] = serde_json::json!(["steering"]);
    }
    Ok(())
}
