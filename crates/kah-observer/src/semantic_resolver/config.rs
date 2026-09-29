/// Borrowed native model preferences, independent of the main agent's choices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactorConfig<'a> {
    model: &'a str,
    reasoning: &'a str,
}

impl Default for CompactorConfig<'_> {
    fn default() -> Self {
        Self {
            model: "gpt-6-luna",
            reasoning: "low",
        }
    }
}

impl<'a> CompactorConfig<'a> {
    pub fn new(model: &'a str, reasoning: Option<&'a str>) -> Result<Self, String> {
        let model = match model {
            "luna" => "gpt-6-luna",
            "sol" => "gpt-6-sol",
            "terra" => "gpt-5.6-terra",
            other => other,
        };
        let reasoning = reasoning.unwrap_or("low");
        if !valid_choice(model) || model.starts_with('-') {
            return Err("OBSERVER_MODEL stage=observer_config: model must be a nonempty native model identifier".into());
        }
        if !valid_choice(reasoning) {
            return Err("OBSERVER_EFFORT stage=observer_config: effort must be a nonempty native reasoning value".into());
        }
        // Codex 0.159 advertises model-specific effort strings, not a closed enum.
        Ok(Self { model, reasoning })
    }

    pub fn model(self) -> &'a str {
        self.model
    }
    pub fn reasoning(self) -> &'a str {
        self.reasoning
    }
}

fn valid_choice(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}
