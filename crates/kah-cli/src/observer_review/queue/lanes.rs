//! Lane aging (15 minutes) keeps older lanes from starving; it never times out
//! an active review.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Intent,
    Commit,
    Gate,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Stamp {
    pub at_ms: u64,
    pub lane: Lane,
}
impl Lane {
    pub fn of(wake: &Wake) -> Self {
        match wake.purpose {
            Purpose::Drift | Purpose::Steering => Self::Intent,
            Purpose::Checkpoint if wake.trigger["review_class"] == "commit" => Self::Commit,
            Purpose::Preparation
                if matches!(
                    wake.trigger["preparation"]["purpose"].as_str(),
                    Some("checkpoint" | "gate")
                ) =>
            {
                Self::Gate
            }
            Purpose::Preparation => Self::Intent,
            _ => Self::Gate,
        }
    }
}
pub fn sort(
    jobs: &mut [(PathBuf, Wake)],
    stamps: &BTreeMap<String, Stamp>,
    last: Option<Lane>,
    now: u64,
) -> Result<()> {
    jobs.sort_by_key(|(_, w)| {
        if w.trigger["verification_followup"]["stage"] == "checks_complete" {
            0
        } else {
            1
        }
    });
    let mut heads = BTreeMap::new();
    for (path, wake) in jobs.iter() {
        let stamp = stamps
            .get(path.file_name().unwrap().to_str().unwrap())
            .ok_or("REVIEW_LANE_STAMP stage=queue")?;
        heads.entry(Lane::of(wake)).or_insert(stamp.at_ms);
    }
    let mut eligible = heads.clone();
    if last == Some(Lane::Gate) && heads.keys().any(|lane| *lane != Lane::Gate) {
        eligible.remove(&Lane::Gate);
    }
    let selected = eligible
        .iter()
        .filter(|(_, at)| now.saturating_sub(**at) > 15 * 60 * 1000)
        .min_by_key(|(_, at)| **at)
        .map(|(lane, _)| *lane)
        .or_else(|| next(&eligible, last));
    jobs.sort_by_key(|(_, w)| if Some(Lane::of(w)) == selected { 0 } else { 1 });
    Ok(())
}
fn next(heads: &BTreeMap<Lane, u64>, last: Option<Lane>) -> Option<Lane> {
    let rotation = match last {
        Some(Lane::Intent) => [Lane::Commit, Lane::Gate, Lane::Intent],
        Some(Lane::Commit) => [Lane::Gate, Lane::Intent, Lane::Commit],
        Some(Lane::Gate) => [Lane::Intent, Lane::Commit, Lane::Gate],
        None => [Lane::Gate, Lane::Commit, Lane::Intent],
    };
    rotation.into_iter().find(|lane| heads.contains_key(lane))
}
