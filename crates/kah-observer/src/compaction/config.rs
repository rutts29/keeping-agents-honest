/// Borrowed native model preferences, independent of the main agent's choices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactorConfig<'a> {
    model: &'a str,
    reasoning: &'a str,
    timeout: std::time::Duration,
}

impl Default for CompactorConfig<'_> {
    fn default() -> Self {
        Self {
            model: "gpt-6.1-sol",
            reasoning: "low",
            timeout: std::time::Duration::from_secs(45),
        }
    }
}

impl<'a> CompactorConfig<'a> {
    pub fn new(model: &'a str, reasoning: Option<&'a str>) -> Result<Self, String> {
        let model = match model {
            // Existing native sessions can retain an older observer binding in
            // their environment. Map it to a supported model instead of
            // launching it.
            "luna" | "sol" | "terra" | "gpt-6-luna" | "gpt-6-sol" | "gpt-reserve"
            | "gpt-5.6-terra" | "gpt-5.6-sol" | "gpt-5.6-luna" | "gpt-5.5" => "gpt-6.1-sol",
            "haiku" | "sonnet" | "claude-sonnet-5" | "claude-haiku-4-5-20251001" => {
                "claude-sonnet-5-5"
            }
            "opus" | "claude-opus-5" | "claude-opus-4-8" => "claude-opus-5-5",
            other => other,
        };
        let reasoning = match reasoning {
            Some("none" | "minimal") => Some("low"),
            other => other,
        };
        Self::requested(model, reasoning)
    }

    /// Validate a new user choice without upgrading a below-floor request.
    pub fn requested(model: &'a str, reasoning: Option<&'a str>) -> Result<Self, String> {
        let model = match model {
            "sonnet" => "claude-sonnet-5-5",
            "opus" => "claude-opus-5-5",
            "fable" => "claude-fable-5-1",
            other => other,
        };
        if !matches!(
            model,
            "gpt-6.1-sol"
                | "gpt-6-astra"
                | "claude-haiku-5-5"
                | "claude-sonnet-5-5"
                | "claude-opus-5-5"
                | "claude-fable-5-1"
        ) {
            return Err("OBSERVER_MODEL stage=observer_config: choose gpt-6.1-sol, gpt-6-astra, claude-haiku-5-5 (high effort or more), claude-sonnet-5-5, claude-opus-5-5 or claude-fable-5-1 (permanent model floor)".into());
        }
        let reasoning = reasoning.unwrap_or("low");
        if !matches!(
            reasoning,
            "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
        ) {
            return Err(
                "OBSERVER_EFFORT stage=observer_config: effort must be low or higher".into(),
            );
        }
        if model == "claude-haiku-5-5" && matches!(reasoning, "low" | "medium") {
            return Err("OBSERVER_EFFORT stage=observer_config: claude-haiku-5-5 needs high effort or more (permanent model floor)".into());
        }
        if model.starts_with("claude-") && reasoning == "ultra" {
            return Err(
                "OBSERVER_EFFORT stage=observer_config: Claude effort supports low through max"
                    .into(),
            );
        }
        Ok(Self {
            model,
            reasoning,
            ..Self::default()
        })
    }

    pub fn model(self) -> &'a str {
        self.model
    }
    pub fn reasoning(self) -> &'a str {
        self.reasoning
    }
    pub fn timeout(self) -> std::time::Duration {
        self.timeout
    }
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Result<Self, String> {
        if timeout.is_zero() {
            return Err(
                "OBSERVER_TIMEOUT_CONFIG stage=observer_config: timeout cannot be disabled".into(),
            );
        }
        self.timeout = timeout;
        Ok(self)
    }
}
