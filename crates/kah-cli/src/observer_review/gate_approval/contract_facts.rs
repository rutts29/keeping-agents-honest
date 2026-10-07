//! Trusted capture/check receipts are exact facts, never user risk decisions.
use super::*;
use kah_observer::verification_contract::ReceiptFact;

pub(super) fn verified(request: &Request, proof: &DeliveryProof, facts: &[ReceiptFact]) -> bool {
    !facts.is_empty()
        && request.head.as_deref() == Some(&proof.head)
        && facts.iter().all(|fact| match fact {
            ReceiptFact::HeadSha => proof.fresh_checkout && oid(&proof.head),
            ReceiptFact::TestExit {
                test_index,
                expected,
            } => proof
                .tests
                .get(*test_index)
                .is_some_and(|t| t.commit == proof.head && t.exit_code == Some(*expected)),
            ReceiptFact::TestOutputLine {
                test_index,
                expected,
            } => {
                !expected.contains(['\n', '\r'])
                    && !expected.trim().is_empty()
                    && proof.tests.get(*test_index).is_some_and(|t| {
                        t.commit == proof.head && t.output.lines().any(|line| line == expected)
                    })
            }
            ReceiptFact::TagHead { tag } => tag_head(request, tag, &proof.head),
        })
}
fn tag_head(request: &Request, tag: &str, head: &str) -> bool {
    let name = format!("refs/tags/{tag}");
    request
        .documents
        .iter()
        .filter(|d| d.id == "git" && d.kind == "controller_facts")
        .filter_map(|d| serde_json::from_str::<Value>(&d.text).ok())
        .filter_map(|v| v["tags"].as_str().map(str::to_owned))
        .any(|tags| {
            tags.lines().any(|line| {
                let mut parts = line.split_whitespace();
                parts.next() == Some(head)
                    && parts.next() == Some(name.as_str())
                    && parts.next().is_none()
            })
        })
}
