use super::*;
use kah_observer::decision_units::{Dimension, ReviewUnits};

pub fn checkpoint(request: &Request, units: &ReviewUnits, deferrals: &[Value]) -> Result<()> {
    if request.purpose != super::super::state::Purpose::Checkpoint {
        return Ok(());
    }
    for dimension in [Dimension::Intent, Dimension::Integration, Dimension::Drift] {
        if !units.requirements.iter().any(|r| {
            r.dimension == dimension
                && !matches!(
                    r.judgment,
                    kah_observer::decision_units::Thinking::ControllerFact { .. }
                )
        }) {
            return Err(format!("REVIEW_REQUIREMENT_DIMENSION stage=checkpoint: named {dimension:?} requirements missing").into());
        }
    }
    for r in &units.requirements {
        if let Some(id) = &r.deferral_id
            && !deferrals.iter().any(|d| d["id"] == *id)
        {
            return Err("REVIEW_DEFERRAL_UNKNOWN stage=checkpoint: requirement links an untracked dependency".into());
        }
    }
    for item in deferrals
        .iter()
        .filter(|d| d["introduced_at"] != request.id)
    {
        let id = item["id"]
            .as_str()
            .ok_or("REVIEW_DEFERRAL_ID stage=checkpoint")?;
        if !units
            .requirements
            .iter()
            .any(|r| r.deferral_id.as_deref() == Some(id))
        {
            return Err(format!("REVIEW_DEFERRAL_COVERAGE stage=checkpoint: named requirement for dependency {id} missing").into());
        }
    }
    Ok(())
}
