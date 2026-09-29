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
