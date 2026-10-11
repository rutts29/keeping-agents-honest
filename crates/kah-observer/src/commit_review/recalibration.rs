//! Counterfactual lines over retained data; this never modifies policy.
use super::*;
pub fn report(reviews: &[Review], line: f64) -> Result<Value, String> {
    if !line.is_finite() || !(0.0..=1.0).contains(&line) {
        return Err("COMMIT_REPORT_LINE stage=recalibration".into());
    }
    let mut rows = vec![];
    for review in reviews {
        let would_wake = review.maximum.as_ref().is_some_and(|m| m.noul >= line);
        let native = review.native.as_ref().map(|n| {
            n.rows
                .iter()
                .filter(|r| r.status == crate::commit_check::Status::Violated)
                .map(|r| r.requirement.clone())
                .collect::<Vec<_>>()
        });
        let later = review.checkpoints.iter().map(|(id,rs)|serde_json::json!({
            "checkpoint":id,"violations":rs.iter().filter(|r|r.status==crate::commit_check::Status::Violated).map(|r|r.requirement.clone()).collect::<Vec<_>>()
        })).collect::<Vec<_>>();
        rows.push(
            serde_json::json!({"worker":review.worker,"head":review.head,
            "registered_line":review.policy.line,"maximum":review.maximum,
            "actual_wake":review.requires_native() || review.native.is_some(),"would_wake":would_wake,
            "gap_wake":!review.gaps.is_empty(),
            "native_violations":native,"later_checkpoints":later,
            "unknown_label":review.native.is_none()&&review.checkpoints.is_empty()}),
        );
    }
    Ok(
        serde_json::json!({"candidate_line":line,"rows":rows,"policy_changed":false,
        "adjustment":"Reviewer/user decision on held-out data, not this counterfactual"}),
    )
}
