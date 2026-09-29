#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactorPreset {
    Luna,
    Terra,
    Sol,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactorConfig {
    pub(super) preset: CompactorPreset,
    pub(super) terra_medium: bool,
}

impl Default for CompactorConfig {
    fn default() -> Self {
        Self {
            preset: CompactorPreset::Luna,
            terra_medium: false,
        }
    }
}

impl CompactorConfig {
    pub fn new(preset: &str, reasoning: Option<&str>) -> Result<Self, String> {
        match (preset, reasoning) {
            ("luna", None) => Ok(Self::default()),
            ("terra", None | Some("high")) => Ok(Self {
                preset: CompactorPreset::Terra,
                terra_medium: false,
            }),
            ("terra", Some("medium")) => Ok(Self {
                preset: CompactorPreset::Terra,
                terra_medium: true,
            }),
            ("sol", None) => Ok(Self {
                preset: CompactorPreset::Sol,
                terra_medium: false,
            }),
            ("luna" | "sol", Some(_)) => Err(
                "--reasoning is only supported for the Terra preset (high or medium)".to_owned(),
            ),
            _ => Err("--compactor must be one of: luna, terra, sol".to_owned()),
        }
    }

    #[must_use]
    pub fn model(self) -> &'static str {
        match self.preset {
            CompactorPreset::Luna => "gpt-5.6-luna",
            CompactorPreset::Terra => "gpt-5.6-terra",
            CompactorPreset::Sol => "gpt-5.6-sol",
        }
    }

    #[must_use]
    pub fn reasoning(self) -> &'static str {
        match self.preset {
            CompactorPreset::Luna => "max",
            CompactorPreset::Terra if self.terra_medium => "medium",
            CompactorPreset::Terra | CompactorPreset::Sol => "high",
        }
    }

    #[must_use]
    pub fn preset_name(self) -> &'static str {
        match self.preset {
            CompactorPreset::Luna => "luna",
            CompactorPreset::Terra => "terra",
            CompactorPreset::Sol => "sol",
        }
    }

    #[must_use]
    pub fn reasoning_argument(self) -> Option<&'static str> {
        match self.preset {
            CompactorPreset::Terra if self.terra_medium => Some("medium"),
            CompactorPreset::Terra => Some("high"),
            CompactorPreset::Luna | CompactorPreset::Sol => None,
        }
    }

    #[must_use]
    pub fn disclosure_for_chunks(self, chunks: usize) -> String {
        format!(
            "compactor model: {}\nreasoning: {}\nexpected calls: {chunks}; maximum {} with repair\nuses local Codex authentication and usage allowance; model output may be incomplete or wrong; the operator owns approved intent",
            self.model(),
            self.reasoning(),
            chunks.saturating_mul(2),
        )
    }
}
