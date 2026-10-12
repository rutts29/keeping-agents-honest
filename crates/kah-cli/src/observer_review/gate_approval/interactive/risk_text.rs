//! Risk wording states recorded uncertainty; it never invents a failure cause.
use super::*;
use kah_observer::review_evidence::VerificationRisk;

pub(crate) fn for_report(report: &Report, id: &str) -> Result<VerificationRisk> {
    if report
        .decision_units
        .as_ref()
        .is_some_and(|u| u.version() >= 12)
    {
        return derive(report, id);
    }
    report
        .verification_risks
        .as_ref()
        .and_then(|items| items.iter().find(|r| r.requirement_id == id))
        .cloned()
        .ok_or("REVIEW_FOLLOWUP_RISK_MISSING stage=verification".into())
}

pub(crate) fn derive(report: &Report, id: &str) -> Result<VerificationRisk> {
    let item = report
        .decision_units
        .as_ref()
        .and_then(|u| u.requirements.iter().find(|r| r.id == id))
        .filter(|r| !r.name.trim().is_empty())
        .ok_or("REVIEW_FOLLOWUP_RISK_MISSING stage=verification: no named requirement fact")?;
    if let Some(risk) = report
        .verification_risks
        .as_ref()
        .and_then(|risks| risks.iter().find(|r| r.requirement_id == id))
        .filter(|r| !r.unverified.trim().is_empty() && !r.acceptance_risk.trim().is_empty())
    {
        let mut risk = risk.clone();
        risk.unverified = format!("{}: {}", item.name, risk.unverified);
        return Ok(risk);
    }
    let recorded = serde_json::to_value(&item.judgment)?;
    let reason = recorded["reasoning"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or(
            "REVIEW_FOLLOWUP_RISK_MISSING stage=verification: no recorded uncertainty explanation",
        )?;
    Ok(VerificationRisk {
        requirement_id: id.into(),
        unverified: format!(
            "{} remains unconfirmed by the recorded judgment. Observer reasoning: {reason}",
            item.name
        ),
        acceptance_risk: format!(
            "Acceptance leaves {} unverified on this gate SHA.",
            item.name
        ),
    })
}
