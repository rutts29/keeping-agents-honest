use super::*;
use kah_observer::{
    decision_units::Requirement,
    review_evidence::{Citation, Document},
};
use serde_json::json;
fn fixture() -> (Request, Entry) {
    let plan = Document::new("plan-0", "plan", "docs/PLAN.md", "Keep this behavior.");
    let requirement:Requirement = serde_json::from_value(json!({"id":"behavior","name":"Behavior preserved","dimension":"intent",
        "authority_items":["goal"],"citations":[Citation{source:plan.id.clone(),digest:plan.digest.clone(),quote:plan.text.clone(),occurrence:0}],
        "components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Exact evidence","evidence_citations":[0]}})).unwrap();
    let request = Request {
        id: "review-next".into(),
        worker: "worker".into(),
        purpose: crate::observer_review::state::Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("b".repeat(40)),
        intent: json!({"goal":"Preserve behavior"}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![plan],
        created_ms: 1,
    };
    let evidence = super::capture::fingerprint(&request, &requirement).unwrap();
    let entry = Entry {
        requirement: requirement.id.clone(),
        review: "review-before".into(),
        head: "a".repeat(40),
        evidence,
        confirmed: true,
        always: false,
        verdict: json!({"requirement":requirement,"verified":{"id":"behavior"},"protocol":12}),
    };
    (request, entry)
}
#[test]
fn clean_is_carried_but_gate_always_and_changed_authority_are_fresh() {
    let (mut request, entry) = fixture();
    assert!(candidate(&request, &entry).unwrap().is_some());
    request.intent["goal"] = json!("Different authority");
    assert!(candidate(&request, &entry).unwrap().is_none());
    let (mut request, mut entry) = fixture();
    entry.always = true;
    assert!(candidate(&request, &entry).unwrap().is_none());
    entry.always = false;
    request.trigger = json!({"review_class":"gate"});
    assert!(candidate(&request, &entry).unwrap().is_none());
}
#[test]
fn unverified_legacy_and_disappeared_quote_never_become_clean() {
    let (mut request, mut entry) = fixture();
    entry.verdict.as_object_mut().unwrap().remove("verified");
    assert!(candidate(&request, &entry).unwrap().is_none());
    let (_, entry) = fixture();
    request.documents[0] = Document::new(
        "plan-0",
        "plan",
        "docs/PLAN.md",
        "The previous quoted unit is gone.",
    );
    assert!(candidate(&request, &entry).unwrap().is_none());
}
#[test]
fn request_planning_keeps_carries_on_demand_and_records_dirty_ids() {
    let (mut request, entry) = fixture();
    request.documents.push(Document::new(
        "git",
        "controller_facts",
        "git:project",
        &json!({"component_impact":{"unmapped_paths":["new-area.rs"]}}).to_string(),
    ));
    request.worker = format!("cache-plan-{}", crate::relay_store::nonce());
    let store = crate::observer_review::state::Store::open(&request.worker).unwrap();
    let root = store.directory.join("verdict-cache");
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    crate::observer_review::source_store::save(
        &root,
        "entry.json",
        &serde_json::to_value(&entry).unwrap(),
    )
    .unwrap();
    prepare(&store.directory, &mut request).unwrap();
    assert_eq!(
        request.trigger["cache_plan"]["unattributed_paths"],
        json!(["new-area.rs"])
    );
    assert_eq!(
        request.trigger["cache_plan"]["carried"][0]["id"],
        "behavior"
    );
    let workspace = store.directory.join("fixture-workspace");
    crate::observer_review::source_store::initialize(&store.directory, &workspace).unwrap();
    crate::observer_review::packet_references::scope(&store.directory, &workspace, &mut request)
        .unwrap();
    let cached = request
        .documents
        .iter()
        .find(|d| d.id == "cached-verdicts")
        .unwrap();
    assert_eq!(cached.kind, "lossless_reference");
    assert!(!cached.text.contains("Exact evidence"));
    let original = crate::observer_review::packet_references::original(cached).unwrap();
    assert!(original.text.contains("review-before"));
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn untouched_committed_source_is_checked_from_catalog_and_kept_reference_only() {
    let (mut request, mut entry) = fixture();
    let code = Document::new(
        "commit-file-cached",
        "committed_code",
        "git:current:source.rs",
        "Whole unchanged code.",
    );
    let mut requirement: Requirement =
        serde_json::from_value(entry.verdict["requirement"].clone()).unwrap();
    requirement.citations[0] = Citation {
        source: code.id.clone(),
        digest: code.digest.clone(),
        quote: code.text.clone(),
        occurrence: 0,
    };
    request.documents = vec![code.clone()];
    entry.evidence = super::capture::fingerprint(&request, &requirement).unwrap();
    entry.verdict["requirement"] = serde_json::to_value(requirement).unwrap();
    request.documents = vec![Document::new(
        "whole-tree-catalog",
        "lossless_tree_catalog",
        "git:complete-tree",
        &json!({"head":request.head,"files":[code]}).to_string(),
    )];
    request.worker = format!("cache-code-plan-{}", crate::relay_store::nonce());
    let store = crate::observer_review::state::Store::open(&request.worker).unwrap();
    let root = store.directory.join("verdict-cache");
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    crate::observer_review::source_store::save(
        &root,
        "entry.json",
        &serde_json::to_value(entry).unwrap(),
    )
    .unwrap();
    prepare(&store.directory, &mut request).unwrap();
    assert_eq!(
        request.trigger["cache_plan"]["carried"][0]["id"],
        "behavior"
    );
    let workspace = store.directory.join("fixture-workspace");
    crate::observer_review::source_store::initialize(&store.directory, &workspace).unwrap();
    crate::observer_review::packet_references::scope(&store.directory, &workspace, &mut request)
        .unwrap();
    let source = request
        .documents
        .iter()
        .find(|d| d.id == "commit-file-cached")
        .unwrap();
    assert_eq!(source.kind, "lossless_reference");
    assert_eq!(
        crate::observer_review::packet_references::original(source)
            .unwrap()
            .text,
        "Whole unchanged code."
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn request_candidates_share_one_immutable_source_verifier() {
    let (mut request, entry) = fixture();
    let mut second = entry.clone();
    second.requirement = "second-behavior".into();
    second.verdict["requirement"]["id"] = "second-behavior".into();
    second.verdict["verified"]["id"] = "second-behavior".into();
    request.worker = format!("shared-cache-plan-{}", crate::relay_store::nonce());
    let store = crate::observer_review::state::Store::open(&request.worker).unwrap();
    let root = store.directory.join("verdict-cache");
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    for (name, entry) in [("one.json", entry), ("two.json", second)] {
        crate::observer_review::source_store::save(
            &root,
            name,
            &serde_json::to_value(entry).unwrap(),
        )
        .unwrap();
    }
    prepare(&store.directory, &mut request).unwrap();
    assert_eq!(
        request.trigger["cache_plan"]["carried"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(request.trigger["cache_plan"]["source_digest_checks"], 1);
    assert_eq!(request.trigger["cache_plan"]["source_quote_checks"], 1);
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unmapped_diff_is_named_without_invalidating_unchanged_attributed_evidence() {
    let (mut request, entry) = fixture();
    request.documents.push(Document::new(
        "git",
        "controller_facts",
        "git:project",
        &json!({"component_impact":{"unmapped_paths":["new-area.rs"]}}).to_string(),
    ));
    assert!(candidate(&request, &entry).unwrap().is_some());
}

#[test]
fn a_map_touched_requirement_is_fresh_even_when_its_old_quotes_still_match() {
    let (mut request, entry) = fixture();
    request.documents.push(Document::new(
        "git",
        "controller_facts",
        "git:project",
        &json!({"component_impact":{"unmapped_paths":[],"components":[{
            "name":"behavior","requirements":["behavior"],"changed_paths":["behavior.rs"]
        }]}})
        .to_string(),
    ));
    assert!(candidate(&request, &entry).unwrap().is_none());
}
