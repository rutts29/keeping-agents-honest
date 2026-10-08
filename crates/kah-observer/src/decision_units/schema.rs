use serde_json::{Value, json};
mod thinking;
pub fn schema() -> Value {
    let mut value = object(
        &["version", "requirements"],
        json!({
            "version":{"const":12},
            "requirements":{"type":"array","minItems":1,"items":requirement()},
            "proportion":{"anyOf":[{"type":"null"},proportion()]},
            "steering":{"anyOf":[{"type":"null"},crate::steering_proposal::schema()]}
        }),
    );
    value["$defs"] = json!({"citation":citation(),"indices":{"type":"array","minItems":1,"items":{"type":"integer","minimum":0}}});
    value
}
fn requirement() -> Value {
    object(
        &[
            "id",
            "name",
            "dimension",
            "authority_items",
            "blocker",
            "citations",
            "components",
            "checks",
            "applicability",
            "judgment",
        ],
        json!({
            "id":{"type":"string","pattern":"^[A-Za-z0-9_-]+$"},"name":{"type":"string"},
            "obligation":{"anyOf":[{"type":"null"},object(&["kind","reasoning","evidence_citations"],json!({"kind":{"enum":["executable","non_executable"]},"reasoning":{"type":"string"},"evidence_citations":{"$ref":"#/$defs/indices"}}))]},
            "dimension":{"enum":["intent","integration","deferral","drift","steering"]},
            "authority_items":{"type":"array","minItems":1,"items":{"type":"string"}},
            "blocker":{"enum":[null,"future_implementation","user_decision"]},
            "worker_question":{"type":["integer","null"],"minimum":0},
            "existing_question_id":{"type":["string","null"]},
            "deferral_id":{"type":["string","null"]},
            "citations":citations(),
            "components":{"type":"array","items":object(&["name","role"],json!({"name":{"type":"string"},"role":{"enum":["implementation","test","supporting"]}}))},
            "checks":{"type":"array","items":object(&["name","command_citation","result_citation","sha_citation","sha"],json!({"name":{"type":"string"},"command_citation":{"type":"integer","minimum":0},"result_citation":{"type":"integer","minimum":0},"sha_citation":{"type":"integer","minimum":0},"sha":{"type":"string"}}))},
            "applicability":{"anyOf":[super::applicability::schema(),{"type":"null"}]},
            "judgment":thinking::schema()
        }),
    )
}
fn proportion() -> Value {
    object(
        &[
            "id",
            "name",
            "worst_case_consequence",
            "estimated_recovery_seconds",
            "estimated_recovery_usage",
            "spent_seconds",
            "spent_usage",
            "reasoning",
            "citations",
        ],
        json!({
            "id":{"type":"string"},"name":{"type":"string"},"worst_case_consequence":{"type":"string"},
            "estimated_recovery_seconds":{"type":"number","minimum":0},"estimated_recovery_usage":{"type":"string"},
            "spent_seconds":{"type":"number","minimum":0},"spent_usage":{"type":"string"},"reasoning":{"type":"string"},"citations":citations()
        }),
    )
}
pub(super) fn object(required: &[&str], properties: Value) -> Value {
    json!({"type":"object","additionalProperties":false,"required":required,"properties":properties})
}
fn citations() -> Value {
    json!({"type":"array","minItems":1,"items":{"$ref":"#/$defs/citation"}})
}
fn citation() -> Value {
    object(
        &["source", "digest", "quote"],
        json!({"source":{"type":"string"},"digest":{"type":"string"},"quote":{"type":"string"},"occurrence":{"type":"integer","minimum":0}}),
    )
}
