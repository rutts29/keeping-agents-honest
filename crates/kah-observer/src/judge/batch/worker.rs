use super::*;
mod retry;
pub(super) fn execute(
    batch: &Batch,
    request: &WireRequest,
    transport: &impl Transport,
) -> Result<Receipt> {
    let cache_root = batch.root.join("receipts");
    let _lease = store::lock(&cache_root.join(cache::key(request)))?;
    if let Some(mut saved) = cache::load(&cache_root, request)? {
        store::event(
            &batch.root,
            &serde_json::json!({"kind":"reused","request":request.id,"at_ms":store::now()}),
        )?;
        saved.reused = true;
        return Ok(saved);
    }
    let mut throttles = 0;
    let started = Instant::now();
    loop {
        let ticket = batch.pacer.start(&batch.root, request)?;
        let response = transport.exchange(request);
        batch
            .pacer
            .finish(&batch.root, &ticket, response.as_ref().ok())?;
        if let Err(error) = &response {
            store::event(
                &batch.root,
                &serde_json::json!({"kind":"transport_error",
                "at_ms":store::now(),"request":request,"error":error.to_string()}),
            )?;
        }
        let response = response?;
        if !matches!(response.status, 429 | 529) {
            // Save before aggregate assembly: restart never resends this answer.
            return answered(
                batch,
                request,
                &response,
                started.elapsed().as_millis() as u64,
            );
        }
        let delay = retry::delay(&response.headers, throttles);
        throttles = throttles.saturating_add(1);
        store::event(
            &batch.root,
            &serde_json::json!({"kind":"backoff","request":request.id,
            "at_ms":store::now(),"status":response.status,"wait_ms":delay.as_millis(),
            "headers":response.headers,"raw_response":response.body,"wire_request":request.body}),
        )?;
        std::thread::sleep(delay);
    }
}
fn answered(
    batch: &Batch,
    request: &WireRequest,
    response: &RawResponse,
    elapsed: u64,
) -> Result<Receipt> {
    if matches!(response.status, 402 | 408) || response.status >= 500 {
        // A service HTTP failure is not an answered judgment or reusable receipt.
        store::event(
            &batch.root,
            &serde_json::json!({"kind":"http_error",
            "at_ms":store::now(),"request":request,"response":response}),
        )?;
        return Err(Error {
            code: "JEV_HTTP_STATUS".into(),
            stage: "judge_http",
            status: Some(response.status),
            numeric_detail: None,
        });
    }
    cache::save(&batch.root.join("receipts"), request, response, elapsed)
}
