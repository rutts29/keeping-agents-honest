//! Send immutable ready contexts concurrently; all preparation effects finish in order.
use super::{ready::Ready, *};
use kah_observer::judge::batch::{Batch as Sender, Mode, Outcome, Receipt, Tokens, WireRequest};
pub(crate) fn send(
    home: &Path,
    worker: &str,
    review: &str,
    ready: &[Ready],
) -> Result<Vec<Receipt>> {
    if ready.is_empty() {
        return Ok(vec![]);
    }
    judge::availability::recovery::check(home)?;
    if judge::availability::unavailable(home)? {
        return Err(judge::Error::new("JEV_SERVICE_UNAVAILABLE").into());
    }
    for request in ready {
        judge::validate_request(&request.input, &request.questions)?;
        super::ready::record_retry(request)?;
    }
    let binding = kah_observer::session::Store::read(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?
    .ok_or("PREPARATION_BINDING stage=context_preparation")?
    .binding;
    let calibration = calibration(worker)?;
    let requests = ready
        .iter()
        .map(|r| {
            let body = json!({"model":binding.jev_model,"state":r.input,"questions":r.questions})
                .to_string();
            Ok(WireRequest {
                id: r.request.id.clone(),
                tokens: Tokens::estimate(&body, calibration.as_ref())?,
                body,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let root = Store::directory(worker)?.join("jev-batches").join(review);
    let rate = kah_core::run_store::RunStore::default_state_root()?.join("jev-rate");
    let client = judge::jev::HttpClient::from_home(home)?;
    let outcome = Sender::new(&root, &rate)?.run(&requests, &client, Mode::Concurrent);
    record_availability(home, &outcome)?;
    let outcome = outcome?;
    Ok(outcome.responses)
}
fn record_availability(
    home: &Path,
    outcome: &std::result::Result<Outcome, judge::Error>,
) -> Result<()> {
    match outcome {
        Err(error) => judge::availability::record_error(home, &error.to_string())?,
        Ok(outcome) => {
            for receipt in &outcome.responses {
                match receipt.judgment(0) {
                    Ok(_) => judge::availability::record(home, None)?,
                    Err(error) => judge::availability::record_error(home, &error.to_string())?,
                }
            }
        }
    }
    Ok(())
}
