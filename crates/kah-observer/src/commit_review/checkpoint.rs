//! Carry unchanged direct reviews; no supersession of explicit checkpoint events.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub carried: Vec<(String, NativeVerdict)>,
    pub review_commits: Vec<String>,
    pub interactions: Vec<String>,
}
pub fn plan(reviews: &[Review], authority: &str, files: &BTreeMap<String, String>) -> Plan {
    let mut plan = Plan {
        carried: vec![],
        review_commits: vec![],
        interactions: vec![],
    };
    for review in reviews {
        plan.interactions.push(review.head.clone());
        if let Some(verdict) = &review.native
            && verdict.authority_digest == authority
            && verdict.files.iter().all(|(p, d)| files.get(p) == Some(d))
        {
            plan.carried.push((review.head.clone(), verdict.clone()));
        } else {
            plan.review_commits.push(review.head.clone());
        }
    }
    plan
}
/// A due checkpoint consumes the wake as part of its fresh range; source stays.
pub fn folded(review: &Review, due: Option<&str>) -> Option<Value> {
    (review.requires_native())
        .then(|| {
            due.map(|id| {
                serde_json::json!({
        "head":review.head,"covered_by_checkpoint":id,"woke":review.woke,
        "gaps":review.gaps,"retained":true})
            })
        })
        .flatten()
}
