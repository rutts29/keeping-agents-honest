//! Small transport schema reuses existing typed domains, not a new judge policy.
use serde_json::{Value, json};
pub fn schema() -> Value {
    let mut units = kah_observer::decision_units::schema::schema();
    let properties = &units["properties"]["requirements"]["items"]["properties"];
    let mut decision = json!({"type":"object","additionalProperties":false,
        "required":["id","name","dimension","authority_items","citations","decision","reasoning","due"],
        "properties":{"decision":{"enum":["satisfied","violated","deferred","not_applicable"]},
        "reasoning":{"type":"string"},"due":{"type":"boolean"},"unconditional":{"type":"boolean"},
        "topic":{"enum":["intent","integration","drift","steering","claim"]},
        "obligation":{"enum":["executable","non_executable"]},"timing_reasoning":{"type":"string"},
        "timing_citations":{"type":"array","items":{"type":"integer","minimum":0}},
        "evidence_citations":{"type":"array","items":{"type":"integer","minimum":0}}}});
    for key in [
        "id",
        "name",
        "dimension",
        "authority_items",
        "citations",
        "components",
        "checks",
        "deferral_id",
        "judgment",
        "existing_question_id",
        "worker_question",
    ] {
        decision["properties"][key] = properties[key].clone();
    }
    decision["properties"]["controller_trigger"] =
        properties["applicability"]["anyOf"][0]["properties"]["controller_trigger"].clone();
    let report = kah_observer::review_evidence::Report::schema();
    decision["properties"]["deferral"] =
        report["properties"]["findings"]["items"]["properties"]["deferral"].clone();
    json!({"type":"object","additionalProperties":false,
        "required":["request_id","format","decisions"],
        "$defs":units.as_object_mut().unwrap().remove("$defs").unwrap(),
        "properties":{"request_id":{"type":"string"},"format":{"const":"commit-light-v2"},
            "decisions":{"type":"array","minItems":1,"items":decision},
            "reminder":report["properties"]["reminder"],
            "proportion":units["properties"]["proportion"],
            "steering":units["properties"]["steering"]}})
}
