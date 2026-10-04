use serde_json::{Value, json};
mod thinking;
pub fn schema() -> Value {
    let mut value = object(
        &["requirements"],
        json!({
            "requirements":{"type":"array","minItems":1,"items":requirement()},
            "proportion":{"anyOf":[{"type":"null"},proportion()]}
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
            "citations",
            "components",
            "checks",
            "judgment",
        ],
        json!({
            "id":{"type":"string","pattern":"^[A-Za-z0-9_-]+$"},"name":{"type":"string"},
            "dimension":{"enum":["intent","integration","deferral","drift","steering"]},
            "authority_items":{"type":"array","minItems":1,"items":{"type":"string"}},
            "citations":citations(),
            "components":{"type":"array","items":object(&["name","role"],json!({"name":{"type":"string"},"role":{"enum":["implementation","test","supporting"]}}))},
            "checks":{"type":"array","items":object(&["name","command_citation","result_citation","sha_citation","sha"],json!({"name":{"type":"string"},"command_citation":{"type":"integer","minimum":0},"result_citation":{"type":"integer","minimum":0},"sha_citation":{"type":"integer","minimum":0},"sha":{"type":"string"}}))},
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
