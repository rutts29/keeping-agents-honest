use super::*;

#[derive(Debug)]
pub struct SourceError {
    pub code: &'static str,
    pub stage: &'static str,
    pub(super) message: String,
    pub(super) blocked: bool,
}

impl SourceError {
    pub(super) fn new(code: &'static str, stage: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            stage,
            message: message.into(),
            blocked: false,
        }
    }

    pub(super) fn blocked(
        code: &'static str,
        stage: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            stage,
            message: message.into(),
            blocked: true,
        }
    }

    pub fn is_blocked(&self) -> bool {
        self.blocked
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} stage={}: {}",
            self.code, self.stage, self.message
        )
    }
}

impl std::error::Error for SourceError {}
