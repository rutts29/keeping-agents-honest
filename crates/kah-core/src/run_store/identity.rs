use super::*;

/// An opaque run identifier accepted as a single state-directory component.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(String);

impl RunId {
    /// Parse an opaque run identifier without accepting path syntax.
    pub fn parse(value: impl Into<String>) -> Result<Self, RunStoreError> {
        let value = value.into();
        let valid_length = (1..=64).contains(&value.len());
        let valid_first = value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric);
        let valid_characters = value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if valid_length && valid_first && valid_characters {
            Ok(Self(value))
        } else {
            Err(RunStoreError::InvalidRunId)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RunId {
    type Error = RunStoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for RunId {
    type Error = RunStoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl AsRef<str> for RunId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
