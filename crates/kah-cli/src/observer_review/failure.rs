//! Recovery follows the failing boundary; reason codes remain stable diagnostics.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    ReportValidation,
    SelectionValidation,
    #[default]
    Host,
    Delivery,
}
#[derive(Debug)]
struct Failure {
    origin: Origin,
    error: Box<dyn Error>,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl Error for Failure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error.as_ref())
    }
}
pub fn validation<T>(origin: Origin, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    operation().map_err(|error| {
        if error.is::<Failure>() {
            error
        } else {
            Box::new(Failure { origin, error }) as Box<dyn Error>
        }
    })
}
pub fn origin(error: &(dyn Error + 'static)) -> Origin {
    if pending(&error) || super::provider_block::reason(&error) {
        return Origin::Host;
    }
    let mut cause = Some(error);
    while let Some(error) = cause {
        if error.downcast_ref::<kah_observer::judge::Error>().is_some()
            || error.downcast_ref::<std::io::Error>().is_some()
        {
            return Origin::Host;
        }
        cause = error.source();
    }
    error
        .downcast_ref::<Failure>()
        .map(|f| f.origin)
        .unwrap_or_default()
}
pub fn correctable(origin: Origin, error: &str) -> bool {
    if pending(&error) || super::provider_block::reason(&error) || error.starts_with("JEV_") {
        return false;
    }
    if !matches!(
        origin,
        Origin::ReportValidation | Origin::SelectionValidation
    ) {
        return false;
    }
    let code = error.split_whitespace().next().unwrap_or(error);
    ![
        "REVIEW_REQUEST_MISMATCH",
        "REVIEW_REQUEST_ID",
        "REVIEW_SOURCE_DIGEST",
        "REVIEW_COMPLETION_RECEIPT_BINDING",
        "REVIEW_CONTROLLER_FACT_BINDING",
        "REVIEW_QUESTION_REFERENCE_SCOPE",
    ]
    .contains(&code)
        && !code.starts_with("REVIEW_SESSION_")
        && !code.starts_with("REVIEW_HOOK_SCOPE")
}

/// Writer contention preserves the request; it is never report correction work.
pub fn pending(error: &dyn fmt::Display) -> bool {
    matches!(
        error.to_string().split_whitespace().next(),
        Some("STORE_BUSY" | "REVIEW_REPORT_REVISION_SUPERSEDED" | "REVIEW_REPORT_PENDING")
    )
}
