use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Domain {
    Mine,
    Research,
    Untrusted,
}

impl Domain {
    pub const ALL: [Self; 3] = [Self::Mine, Self::Research, Self::Untrusted];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Research => "research",
            Self::Untrusted => "untrusted",
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub code: &'static str,
    pub stage: &'static str,
    pub detail: String,
}

impl Error {
    pub(crate) fn new(code: &'static str, stage: &'static str, detail: impl ToString) -> Self {
        Self {
            code,
            stage,
            detail: detail.to_string(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} stage={}: {}", self.code, self.stage, self.detail)
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, Default)]
pub struct LaunchOptions {
    pub online: bool,
    pub yolo: bool,
}

impl LaunchOptions {
    pub fn validate(self, domain: Domain) -> Result<(), Error> {
        if self.online && domain != Domain::Research {
            return Err(Error::new(
                "ONLINE_DOMAIN",
                "policy",
                "--online requires research",
            ));
        }
        if self.yolo && domain != Domain::Mine {
            return Err(Error::new("YOLO_DOMAIN", "policy", "--yolo requires mine"));
        }
        Ok(())
    }
}
