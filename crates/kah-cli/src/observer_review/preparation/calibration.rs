use super::*;
pub(crate) fn calibration(
    worker: &str,
) -> Result<Option<kah_observer::context_budget::Calibration>> {
    let mut calibration: Option<kah_observer::context_budget::Calibration> =
        crate::observer_review::state::read(
            &Store::directory(worker)?.join("token-calibration.json"),
        )?;
    if calibration.is_none() {
        let source = kah_observer::session::Store::read(
            &kah_core::run_store::RunStore::default_state_root()?,
            worker,
        )?;
        calibration = source.and_then(|s| {
            s.judgments.iter().rev().find_map(|j| {
                j.metrics
                    .input_utf8_bytes
                    .map(|bytes| kah_observer::context_budget::Calibration {
                        input_utf8_bytes: bytes,
                        input_tokens: j.metrics.input_tokens,
                    })
            })
        });
    }
    Ok(calibration)
}
