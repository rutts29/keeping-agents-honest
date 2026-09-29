use super::*;

#[derive(Debug)]
pub struct VerificationError {
    pub code: &'static str,
    pub stage: &'static str,
    message: String,
}

impl VerificationError {
    pub(super) fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            stage: "verification",
            message: message.into(),
        }
    }

    pub(super) fn io(action: &str, error: std::io::Error) -> Self {
        Self::invalid("VERIFICATION_IO", format!("{action}: {error}"))
    }
}

impl fmt::Display for VerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} stage={}: {}",
            self.code, self.stage, self.message
        )
    }
}

impl std::error::Error for VerificationError {}
