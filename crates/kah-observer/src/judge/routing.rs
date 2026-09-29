use super::*;
use crate::semantic_resolver::CompactorConfig;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Jev,
    #[default]
    Luna,
}
impl Backend {
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "jev" => Ok(Self::Jev),
            "luna" => Ok(Self::Luna),
            _ => Err(Error::new("JUDGE_BACKEND")),
        }
    }
    pub fn for_domain(domain: &str, explicit: Option<&str>) -> Result<Self, Error> {
        match explicit {
            Some(value) => Self::parse(value),
            None if domain == "mine" => Ok(Self::Jev),
            None => Ok(Self::Luna),
        }
    }
}

pub fn evaluate(
    home: &Path,
    root: &Path,
    backend: Backend,
    config: CompactorConfig<'_>,
    state: &Value,
    questions: &Questions,
) -> Result<Judgment, Error> {
    let baseline = luna::Luna { root, config };
    if backend == Backend::Luna {
        return baseline.evaluate(state, questions);
    }
    let result = jev::HttpClient::from_home(home).and_then(|client| {
        jev::Jev {
            client,
            model: "jev-latest".into(),
        }
        .evaluate(state, questions)
    });
    match result {
        Ok(result) => Ok(result),
        Err(error) => {
            let mut fallback = baseline.evaluate(state, questions)?;
            fallback.fallback_reason = Some(error.to_string());
            Ok(fallback)
        }
    }
}
