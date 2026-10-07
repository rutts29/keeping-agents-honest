use serde_json::{Value, json};

pub fn verify(replay: &Value, stages: &[Value]) -> Result<(), &'static str> {
    let actual = replay.as_array().ok_or("TRACK2_REPLAY_MISSING")?;
    if actual.len() != 2
        || actual[0]["counterfactual"] != false
        || actual[1]["counterfactual"] != true
    {
        return Err("TRACK2_REPLAY_CHRONOLOGY");
    }
    for entry in actual {
        complete(&entry["result"])?;
    }
    if actual[1]["result"]["assessment"]["passed"] != false
        || !confirmed_unit(&actual[1]["result"], "drift", "violated")
    {
        return Err("TRACK2_COUNTERFACTUAL_NOT_CAUGHT");
    }
    if stages.len() != 3 {
        return Err("TRACK2_STAGES_MISSING");
    }
    for (index, stage) in stages.iter().enumerate() {
        checkpoint(stage, index, &stages[0])?;
    }
    if stages[0]["deferrals"].as_array().is_none_or(Vec::is_empty)
        || stages[0]["probe"]["serializer_absent"] != true
        || !confirmed_unit(&stages[0]["result"], "deferral", "deferred")
    {
        return Err("TRACK2_GENUINE_NOT_RECORDED");
    }
    if !confirmed_unit(&stages[1]["result"], "deferral", "violated")
        || !confirmed_unit(&stages[1]["result"], "integration", "violated")
    {
        return Err("TRACK2_FAKE_OR_INTEGRATION_UNCONFIRMED");
    }
    if stages[1]["native_nudge"] != true || !stages[1]["nudge_receipt"].is_object() {
        return Err("TRACK2_NUDGE_MISSING");
    }
    if !stages[2]["deferrals"].as_array().is_some_and(Vec::is_empty)
        || stages[2]["result"]["head"] == stages[1]["result"]["head"]
    {
        return Err("TRACK2_REPAIR_NOT_PROVEN");
    }
    Ok(())
}
fn checkpoint(stage: &Value, index: usize, first: &Value) -> Result<(), &'static str> {
    complete(&stage["result"])?;
    if stage["result"]["assessment"]["passed"] != json!(index != 1) {
        return Err("TRACK2_STAGE_VERDICT");
    }
    for key in ["observer_session", "pane"] {
        if stage["result"][key].as_str().is_none_or(str::is_empty)
            || stage["result"][key] != first["result"][key]
        {
            return Err("TRACK2_OBSERVER_CHANGED");
        }
    }
    if stage["result"]["head"].as_str().is_none_or(str::is_empty)
        || stage["result"]["head"] != stage["probe"]["sha"]
    {
        return Err("TRACK2_SHA_MISMATCH");
    }
    if stage["probe"]["phase1"] != true
        || (index > 0
            && (stage["probe"]["phase2"] != true
                || stage["probe"]["integration"] != json!(index == 2)))
    {
        return Err("TRACK2_PROBE_FAILED");
    }
    if stage["phase3_started"] != false {
        return Err("TRACK2_PREMATURE_PHASE3");
    }
    Ok(())
}
fn confirmed_unit(result: &Value, dimension: &str, decision: &str) -> bool {
    let Ok(judgment) =
        serde_json::from_value::<kah_observer::judge::Judgment>(result["judgment"].clone())
    else {
        return false;
    };
    result["report"]["decision_units"]["requirements"]
        .as_array()
        .is_some_and(|units| {
            units.iter().any(|unit| {
                unit["dimension"] == dimension
                    && unit["judgment"]["kind"] == "semantic"
                    && unit["judgment"]["decision"] == decision
                    && unit["id"].as_str().is_some_and(|id| {
                        kah_observer::decision_units::confirmed(
                            &judgment,
                            &format!("requirement.{id}"),
                            "supported",
                        )
                    })
            })
        })
}
fn complete(result: &Value) -> Result<(), &'static str> {
    if result["request"].as_str().is_none_or(str::is_empty)
        || !result["report"].is_object()
        || result["judgment"]["backend"] != "jev"
        || result["judgment"]["metrics"]["calls"]
            .as_u64()
            .is_none_or(|n| n == 0)
        || !result["usage_gap"].is_null()
        || result["native_tokens"].as_u64().is_none_or(|n| n == 0)
        || !result["assessment"]["passed"].is_boolean()
        || ["needs_user", "needs_thinker"].iter().any(|key| {
            result["assessment"][key]
                .as_array()
                .is_some_and(|a| !a.is_empty())
        })
    {
        return Err("TRACK2_RECEIPT_INCOMPLETE");
    }
    Ok(())
}
