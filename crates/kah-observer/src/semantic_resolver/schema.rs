use super::*;

pub(super) fn proposal_schema() -> Value {
    let source_ids = json!({"type":"array","items":{"type":"string"}});
    let alternative = json!({
        "type":"object",
        "properties": {
            "key":{"type":"string"},
            "text":{"type":"string"},
            "source_record_ids":source_ids,
        },
        "required":["key","text","source_record_ids"],
        "additionalProperties":false,
    });
    let operation = json!({
        "type":"object",
        "properties": {
            "operation":{"type":"string","enum":["add","supersede","withdraw","conflict"]},
            "key":{"type":"string"},
            "kind":{"type":"string","enum":["outcome","constraint","invariant","scope","validation","stop_condition"]},
            "text":{"type":"string"},
            "target_key":{"type":"string"},
            "intent_keys":{"type":"array","items":{"type":"string"}},
            "source_record_ids":source_ids,
            "alternatives":{"type":"array","items":alternative},
        },
        "required":["operation","key","kind","text","target_key","intent_keys","source_record_ids","alternatives"],
        "additionalProperties":false,
    });
    json!({
        "$schema":"https://json-schema.org/draft/2020-12/schema",
        "type":"object",
        "properties": {
            "schema_version":{"type":"integer","enum":[PROPOSAL_SCHEMA_VERSION]},
            "goal":{
                "type":"object",
                "properties":{"text":{"type":"string"},"source_record_ids":source_ids},
                "required":["text","source_record_ids"],
                "additionalProperties":false,
            },
            "accounted_source_record_ids":source_ids,
            "operations":{"type":"array","items":operation},
        },
        "required":["schema_version","goal","accounted_source_record_ids","operations"],
        "additionalProperties":false,
    })
}

pub(super) fn incremental_proposal_schema() -> Value {
    let source_ids = json!({"type":"array","items":{"type":"string"}});
    let alternative = json!({
        "type":"object",
        "properties":{
            "key":{"type":"string"},
            "text":{"type":"string"},
            "source_record_ids":source_ids,
        },
        "required":["key","text","source_record_ids"],
        "additionalProperties":false,
    });
    let operation = json!({
        "type":"object",
        "properties":{
            "operation":{"type":"string","enum":["add","supersede","withdraw","conflict","reopen"]},
            "key":{"type":"string"},
            "kind":{"type":"string","enum":["outcome","constraint","invariant","scope","validation","stop_condition"]},
            "text":{"type":"string"},
            "target_intent_id":{"type":"string"},
            "intent_ids":{"type":"array","items":{"type":"string"}},
            "evidence_id":{"type":"string"},
            "reason":{"type":"string"},
            "source_record_ids":source_ids,
            "alternatives":{"type":"array","items":alternative},
        },
        "required":["operation","key","kind","text","target_intent_id","intent_ids","evidence_id","reason","source_record_ids","alternatives"],
        "additionalProperties":false,
    });
    let proposed_goal = json!({
        "type":"object",
        "properties":{
            "text":{"type":"string"},
            "source_record_ids":source_ids,
        },
        "required":["text","source_record_ids"],
        "additionalProperties":false,
    });
    json!({
        "$schema":"https://json-schema.org/draft/2020-12/schema",
        "type":"object",
        "properties":{
            "schema_version":{"type":"integer","enum":[PROPOSAL_SCHEMA_VERSION]},
            "base_projection_revision":{"type":"integer","minimum":1},
            "base_event_sequence":{"type":"integer","minimum":1},
            "classification":{"type":"string","enum":["no_change","additive","supersession","withdrawal","conflict","reopen","goal_change"]},
            "accounted_active_intent_ids":{"type":"array","items":{"type":"string"}},
            "accounted_source_record_ids":source_ids,
            "operations":{"type":"array","items":operation},
            "proposed_goal":{"anyOf":[proposed_goal,{"type":"null"}]},
        },
        "required":["schema_version","base_projection_revision","base_event_sequence","classification","accounted_active_intent_ids","accounted_source_record_ids","operations","proposed_goal"],
        "additionalProperties":false,
    })
}
