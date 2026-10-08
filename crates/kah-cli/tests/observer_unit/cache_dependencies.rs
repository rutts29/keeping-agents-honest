use super::*;
use kah_observer::{
    decision_units::{CheckReceipt, Component, ComponentRole, Requirement},
    review_evidence::{Citation, Document},
};
fn fixture() -> (Request, Requirement) {
    let plan = Document::new("plan-0", "plan", "docs/PLAN.md", "Implement the parser.");
    let mut r: Requirement = serde_json::from_value(json!({"id":"parser","name":"Parser","dimension":"integration",
        "authority_items":["goal"],"citations":[],"components":[],"checks":[],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Evidence","evidence_citations":[0]}})).unwrap();
    r.citations.push(Citation {
        source: plan.id.clone(),
        digest: plan.digest.clone(),
        quote: plan.text.clone(),
        occurrence: 0,
    });
    let request = Request {
        id: "review-dependency".into(),
        worker: "worker".into(),
        purpose: crate::observer_review::state::Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":"Parser"}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![plan],
        created_ms: 1,
    };
    (request, r)
}
#[test]
fn only_cited_plan_documents_are_dependencies() {
    let (mut request, r) = fixture();
    let original = fingerprint(&request, &r).unwrap();
    request.documents.push(Document::new(
        "plan-1",
        "plan",
        "docs/STATUS.md",
        "Unrelated work status changed.",
    ));
    assert_eq!(fingerprint(&request, &r).unwrap(), original);
    request.documents[0] = Document::new(
        "plan-0",
        "plan",
        "docs/PLAN.md",
        "Different parser requirement.",
    );
    assert_ne!(fingerprint(&request, &r).unwrap(), original);
}
#[test]
fn component_dependency_uses_native_blob_digest_not_just_its_name() {
    let (mut request, mut r) = fixture();
    r.components.push(Component {
        name: "parser".into(),
        role: ComponentRole::Implementation,
    });
    request.documents.push(Document::new("git","controller_facts","git:context",
        &json!({"component_map_binding":{"map_digest":"checked-declaration"},"component_impact":{"declaration_digest":"checked-declaration","components":[{"name":"parser","digest":"native-blob-one","requirements":["parser"]}]}}).to_string()));
    let first = fingerprint(&request, &r).unwrap();
    assert_eq!(
        first.components["parser"],
        digest(&json!({"native":"native-blob-one","role":"implementation"})).unwrap()
    );
    request.documents[1] = Document::new("git","controller_facts","git:context",
        &json!({"component_map_binding":{"map_digest":"checked-declaration"},"component_impact":{"declaration_digest":"checked-declaration","components":[{"name":"parser","digest":"native-blob-two","requirements":["parser"]}]}}).to_string());
    assert_ne!(first, fingerprint(&request, &r).unwrap());
    request.documents.pop();
    assert!(
        fingerprint(&request, &r)
            .unwrap_err()
            .to_string()
            .contains("REVIEW_CACHE_COMPONENT")
    );
    let (retained, gap) = snapshot(&request, &r).unwrap();
    assert!(gap.unwrap().contains("REVIEW_CACHE_COMPONENT"));
    assert!(retained.components.is_empty());
    assert!(retained.authority.contains_key("goal"));
}
#[test]
fn check_dependencies_include_exact_native_command_result_and_sha_quotes() {
    let (mut request, mut r) = fixture();
    let native = Document::new(
        "transcript",
        "worker_transcript",
        "native:worker",
        "cargo test parser\nPASS\nSHA\n",
    );
    for quote in ["cargo test parser", "PASS", "SHA"] {
        r.citations.push(Citation {
            source: native.id.clone(),
            digest: native.digest.clone(),
            quote: quote.into(),
            occurrence: 0,
        });
    }
    request.documents.push(native);
    r.checks.push(CheckReceipt {
        name: "parser check".into(),
        command_citation: 1,
        result_citation: 2,
        sha_citation: 3,
        sha: "a".repeat(40),
    });
    let original = fingerprint(&request, &r).unwrap();
    r.citations[2].quote = "FAIL".into();
    request.documents[1] = Document::new(
        "transcript",
        "worker_transcript",
        "native:worker",
        "cargo test parser\nFAIL\nSHA\n",
    );
    for citation in r.citations.iter_mut().skip(1) {
        citation.digest = request.documents[1].digest.clone();
    }
    assert_ne!(original.tests, fingerprint(&request, &r).unwrap().tests);
}
