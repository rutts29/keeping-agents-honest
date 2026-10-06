//! Consume only controller-provided verification and artifact receipts. A
//! model's claim or marker text cannot create a passing delivery proof.
use super::*;

pub fn captured(request: &Request) -> Result<Option<DeliveryProof>> {
    let Some(head) = request.head.as_deref() else {
        return Ok(None);
    };
    let mut tests = vec![];
    for event in request.trigger["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["kind"] == "run_completion")
    {
        let receipt: kah_run::verification::Receipt =
            serde_json::from_value(event["verification"].clone())?;
        if event["completion_key"]["commit"] != head || receipt.commit != head {
            return Err("REVIEW_GATE_VERIFICATION_SHA stage=approval".into());
        }
        tests.push(TestReceipt {
            command: receipt.command,
            commit: receipt.commit,
            exit_code: receipt.exit_code,
            success: receipt.success,
            output: receipt.output,
        });
    }
    if tests.is_empty() {
        return Ok(None);
    }
    let artifacts = request
        .documents
        .iter()
        .filter(|d| d.kind == "controller_artifact_digests")
        .map(|d| serde_json::from_str(&d.text).map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(DeliveryProof {
        head: head.into(),
        fresh_checkout: true,
        tests,
        artifacts,
    }))
}
