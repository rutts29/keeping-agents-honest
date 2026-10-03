use kah_observer::context_selection::{self, Manifest, Selection};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn whole_items_are_selected_with_accounting_and_user_authority() {
    let state = json!({"intent":{"goal":" Original goal\r\n完整 words ","items":[{"id":"keep","text":"Do not commit notes/.","origin":"relayed"}]},"event":{"actions":[{"id":"a","output":"Whole output\nlast line\n"},{"id":"b","output":"Irrelevant output."}]}});
    let items = context_selection::items(&state, &BTreeSet::from(["keep".into()]));
    let mut manifest = Manifest {
        decision: "drift-1".into(),
        selected: vec![],
        excluded: vec![],
    };
    for item in &items {
        let choice = Selection {
            id: item.id.clone(),
            digest: item.digest.clone(),
            reason: "Observer relevance decision".into(),
            superseded_by: None,
        };
        if item.pointer.starts_with("/event/actions/1/") {
            manifest.excluded.push(choice);
        } else {
            manifest.selected.push(choice);
        }
    }
    let prepared =
        context_selection::prepare("drift-1", &state, &items, &manifest, &BTreeMap::new()).unwrap();
    assert_eq!(prepared["intent"], state["intent"]);
    assert_eq!(prepared["event"]["actions"].as_array().unwrap().len(), 1);
    assert_eq!(
        prepared["event"]["actions"][0],
        state["event"]["actions"][0]
    );
    let protected = manifest
        .selected
        .iter()
        .position(|s| s.id == "/intent/items/0")
        .unwrap();
    manifest.excluded.push(manifest.selected.remove(protected));
    assert!(
        context_selection::prepare("drift-1", &state, &items, &manifest, &BTreeMap::new())
            .unwrap_err()
            .starts_with("CONTEXT_USER_AUTHORITY")
    );
}

#[test]
fn whole_source_context_can_be_excluded_without_cutting_the_verified_quote_or_user_items() {
    let state = json!({"intent":{"goal":"Original goal","items":[{"id":"active","text":"Never truncate my words.","source":[{"role":"user","origin":"relayed"}]}]},"event":{"actions":[{"evidence":{"citations":[{"quote":"Whole relevant paragraph.\n","context":"Unrelated full source.\n".repeat(10000)}]}}]}});
    let items = context_selection::items(&state, &BTreeSet::from(["active".into()]));
    let mut manifest = Manifest {
        decision: "one".into(),
        selected: vec![],
        excluded: vec![],
    };
    for item in &items {
        let selection = Selection {
            id: item.id.clone(),
            digest: item.digest.clone(),
            reason: "Native observer selected relevant whole fields".into(),
            superseded_by: None,
        };
        if item.pointer.ends_with("/context") {
            manifest.excluded.push(selection);
        } else {
            manifest.selected.push(selection);
        }
    }
    let prepared =
        context_selection::prepare("one", &state, &items, &manifest, &BTreeMap::new()).unwrap();
    assert_eq!(prepared["intent"], state["intent"]);
    assert_eq!(
        prepared["event"]["actions"][0]["evidence"]["citations"][0]["quote"],
        "Whole relevant paragraph.\n"
    );
    assert!(
        prepared["event"]["actions"][0]["evidence"]["citations"][0]
            .get("context")
            .is_none()
    );
}

#[test]
fn missing_items_and_forged_digests_cannot_be_silently_omitted() {
    let input = json!({"goal":"whole goal","actions":["first","second"]});
    let items = context_selection::items(&input, &BTreeSet::new());
    let mut manifest = Manifest {
        decision: "one".into(),
        selected: items
            .iter()
            .map(|i| Selection {
                id: i.id.clone(),
                digest: i.digest.clone(),
                reason: "Relevant".into(),
                superseded_by: None,
            })
            .collect(),
        excluded: vec![],
    };
    manifest.selected.pop();
    assert!(
        context_selection::prepare("one", &input, &items, &manifest, &BTreeMap::new())
            .unwrap_err()
            .starts_with("CONTEXT_UNACCOUNTED_ITEMS")
    );
    manifest.selected[0].digest = "forged".into();
    assert!(
        context_selection::prepare("one", &input, &items, &manifest, &BTreeMap::new())
            .unwrap_err()
            .starts_with("CONTEXT_ACCOUNTING")
    );
}
