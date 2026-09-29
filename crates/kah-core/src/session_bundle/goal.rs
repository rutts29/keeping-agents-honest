use super::*;

/// The observed state of a provider-native goal at intake time.
///
/// `Unknown` is distinct from `Absent`: it means the provider goal has not
/// been observed. Only `Known` carries goal text.
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum NativeGoal {
    Known { text: String },
    Absent,
    Unknown,
}

impl NativeGoal {
    pub fn known(text: impl Into<String>) -> Result<Self, SessionBundleError> {
        let text = text.into();
        validate_native_goal_text(&text)?;
        Ok(Self::Known { text })
    }

    #[must_use]
    pub fn state(&self) -> &'static str {
        match self {
            Self::Known { .. } => "known",
            Self::Absent => "absent",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Known { text } => Some(text),
            Self::Absent | Self::Unknown => None,
        }
    }

    pub(super) fn validate(&self) -> Result<(), SessionBundleError> {
        if let Self::Known { text } = self {
            validate_native_goal_text(text)?;
        }
        Ok(())
    }
}

impl fmt::Debug for NativeGoal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeGoal")
            .field("state", &self.state())
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireNativeGoal {
    state: String,
    #[serde(default)]
    text: Option<Option<String>>,
}

impl<'de> Deserialize<'de> for NativeGoal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = WireNativeGoal::deserialize(deserializer)?;
        let native_goal = match (wire.state.as_str(), wire.text) {
            ("known", Some(Some(text))) => NativeGoal::Known { text },
            ("absent", None) => NativeGoal::Absent,
            ("unknown", None) => NativeGoal::Unknown,
            ("known", _) => {
                return Err(serde::de::Error::custom(
                    "native_goal known state requires text",
                ));
            }
            ("absent" | "unknown", _) => {
                return Err(serde::de::Error::custom(
                    "native_goal text is only allowed for known state",
                ));
            }
            _ => return Err(serde::de::Error::custom("unsupported native_goal state")),
        };
        native_goal.validate().map_err(serde::de::Error::custom)?;
        Ok(native_goal)
    }
}
