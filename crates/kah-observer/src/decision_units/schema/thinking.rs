use super::*;
pub(super) fn schema() -> Value {
    json!({"oneOf":[
        object(&["kind","fact","evidence_citations"],json!({"kind":{"const":"controller_fact"},"fact":super::super::controller_fact::schema(),"evidence_citations":{"$ref":"#/$defs/indices"}})),
        object(&["kind","fact","evidence_citations"],json!({"kind":{"const":"artifact_preservation"},"fact":super::super::artifact::schema(),"evidence_citations":{"$ref":"#/$defs/indices"}})),
        object(&["kind","fact","evidence_citations"],json!({"kind":{"const":"atomic_fact"},"fact":super::super::literal::schema(),"evidence_citations":{"$ref":"#/$defs/indices"}})),
        object(&["kind","decision","reasoning","evidence_citations"],json!({"kind":{"const":"semantic"},"decision":{"enum":["satisfied","violated","deferred","not_applicable"]},"reasoning":{"type":"string"},"evidence_citations":{"$ref":"#/$defs/indices"}})),
        object(&["kind","reason","summary","question","options","evidence_citations","ledger_review"],json!({
            "kind":{"const":"needs_user"},"reason":{"enum":["complex_context","ambiguous_instructions","contradictory_instructions","job_context"]},"summary":{"type":"string"},"question":{"type":"string"},
            "options":{"type":"array","minItems":2,"items":{"type":"string"}},"evidence_citations":{"$ref":"#/$defs/indices"},
            "ledger_review":object(&["reasoning","citations"],json!({"reasoning":{"type":"string"},"citations":{"$ref":"#/$defs/indices"}}))
        }))
    ]})
}
