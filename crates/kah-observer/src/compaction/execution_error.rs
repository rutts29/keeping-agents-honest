#[derive(Debug)]
pub(super) struct ExecutionFailure {
    pub(super) code: &'static str,
    pub(super) stage: &'static str,
    pub(super) artifact: Option<String>,
}
impl ExecutionFailure {
    pub(super) fn new(code: &'static str, stage: &'static str) -> Self {
        Self {
            code,
            stage,
            artifact: None,
        }
    }
    pub(super) fn artifact(code: &'static str, artifact: String) -> Self {
        Self {
            code,
            stage: "provider_output",
            artifact: Some(artifact),
        }
    }
}
