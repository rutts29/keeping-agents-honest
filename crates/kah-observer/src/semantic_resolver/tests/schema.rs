use super::*;

#[test]
fn incremental_output_schema_requires_every_declared_root_property() {
    let schema = incremental_proposal_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("root schema properties");
    let required = schema["required"]
        .as_array()
        .expect("root schema required")
        .iter()
        .map(|value| value.as_str().expect("required property"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        required,
        properties.keys().map(String::as_str).collect(),
        "Codex structured output requires every property, including nullable properties"
    );
}

#[test]
fn source_identifiers_are_selected_from_real_records_not_retyped() {
    let bundle = delta("real-user-identifier", "Use BETA");
    let schema = constrain_sources(incremental_proposal_schema(), &bundle);
    assert_eq!(
        schema["properties"]["accounted_source_record_ids"]["items"]["enum"],
        json!(["real-user-identifier"])
    );
    assert_eq!(
        schema["properties"]["operations"]["items"]["properties"]["source_record_ids"]["items"]["enum"],
        json!(["real-user-identifier"])
    );
    assert_eq!(
        schema["properties"]["proposed_goal"]["anyOf"][0]["properties"]["source_record_ids"]["items"]
            ["enum"],
        json!(["real-user-identifier"])
    );
}

#[test]
fn incremental_base_numbers_and_target_ids_are_schema_selected() {
    let (history, projection) = accepted_state();
    let schema = constrain_base(incremental_proposal_schema(), &history, &projection);
    assert_eq!(
        schema["properties"]["base_projection_revision"]["enum"],
        json!([projection.revision])
    );
    let choices =
        schema["properties"]["operations"]["items"]["properties"]["target_intent_id"]["enum"]
            .as_array()
            .unwrap();
    assert!(choices.contains(&json!("intent-existing")) && choices.contains(&json!("")));
    assert!(!choices.contains(&json!("invented-intent")));
}
