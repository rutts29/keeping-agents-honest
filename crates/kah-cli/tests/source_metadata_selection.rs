use kah_cli::observer_review::{
    source_store::{self, Select},
    state::Store,
};
use serde_json::json;
#[test]
fn streamed_metadata_keeps_requested_units_and_does_not_resolve_body_sources() {
    let worker = format!("selected-source-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let data = json!({"worker":"worker","documents":[{"kind":"worker_transcript","text":"Whole history. ".repeat(20000),"native_records":[{"context":{"record_id":"record-one","text":"Whole unrelated tool result. ".repeat(20000)}}]}]});
    source_store::save(&store.directory, "request.json", &data).unwrap();
    let selection = Select::fields(&[
        ("worker", Select::All),
        (
            "documents",
            Select::Each(Box::new(Select::fields(&[
                ("kind", Select::All),
                (
                    "native_records",
                    Select::Each(Box::new(Select::fields(&[(
                        "context",
                        Select::fields(&[("record_id", Select::All)]),
                    )]))),
                ),
            ]))),
        ),
    ]);
    let selected =
        source_store::read_selected(&store.directory.join("request.json"), &selection).unwrap();
    assert_eq!(
        selected,
        json!({"worker":"worker","documents":[{"kind":"worker_transcript","native_records":[{"context":{"record_id":"record-one"}}]}]})
    );
    assert_eq!(
        source_store::read::<serde_json::Value>(&store.directory.join("request.json")).unwrap(),
        data
    );
    let path = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
