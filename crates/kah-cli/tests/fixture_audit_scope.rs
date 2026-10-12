#[path = "terminal_support/audit_scope.rs"]
mod audit_scope;
use audit_scope::{Resource, Scope, report};
use serde_json::json;
use std::path::PathBuf;
fn scope() -> Scope {
    Scope {
        owner: "mine".into(),
        roots: vec!["/private/own".into()],
        active_foreign: vec!["/private/foreign".into()],
    }
}
fn resource(path: &str) -> Resource {
    Resource {
        identity: json!({"pid":7}),
        paths: vec![PathBuf::from(path)],
        owner: None,
    }
}
#[test]
fn an_own_leftover_fails_even_if_it_claims_a_foreign_identity() {
    let mut r = resource("/private/own/worktree");
    r.owner = Some("other".into());
    let result = report(&scope(), vec![r]);
    assert_eq!(result["passed"], false);
}
#[test]
fn an_attributed_foreign_concurrent_resource_does_not_fail_this_run() {
    let result = report(&scope(), vec![resource("/private/foreign/worktree")]);
    assert_eq!(result["passed"], true);
    assert_eq!(
        result["foreign_concurrent_attributed"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn an_unknown_or_prefix_lookalike_resource_still_fails() {
    let result = report(
        &scope(),
        vec![resource("/private/foreign-impostor/worktree")],
    );
    assert_eq!(result["passed"], false);
    assert_eq!(result["unattributed"].as_array().unwrap().len(), 1);
}
