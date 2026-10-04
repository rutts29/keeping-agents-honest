use serde_json::{Value, json};
pub fn schema() -> Value {
    let citations = json!({"type":"array","minItems":1,"items":{"$ref":"#/$defs/citation"}});
    json!({"type":"object","additionalProperties":false,"required":["source_record","directs","kind","supersedes","reasoning","citations","newer_conflicts"],"properties":{
        "source_record":{"type":"string"},"directs":{"type":"boolean"},"kind":{"enum":["initial_task","new_constraint","correction","scope_expansion","scope_reduction","goal_change","question_only","none"]},"supersedes":{"type":["string","null"]},"reasoning":{"type":"string"},"citations":citations,
        "newer_conflicts":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","conflicts","reasoning","citations"],"properties":{"id":{"type":"string"},"conflicts":{"type":"boolean"},"reasoning":{"type":"string"},"citations":citations}}}
    }})
}
