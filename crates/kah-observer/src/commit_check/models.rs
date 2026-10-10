use super::*;
use kah_core::intent_history::History;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    pub text: String,
    pub origin: String,
    pub sources: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentIntent {
    pub goal: String,
    pub items: BTreeMap<String, Authority>,
    pub retired: BTreeSet<String>,
    pub permissions: BTreeMap<String, BTreeSet<String>>,
    pub conflicts: Value,
}
impl CurrentIntent {
    pub fn resolve(
        history: &History,
        permissions: BTreeMap<String, BTreeSet<String>>,
    ) -> Result<Self, String> {
        let projection = kah_core::projection::project(
            history,
            kah_core::projection::ProjectionConfig::default(),
        )
        .map_err(|e| format!("MEASURE_INTENT stage=resolve: {e}"))?;
        let snapshot = history.snapshot();
        let mut items: BTreeMap<String, Authority> = snapshot
            .intents
            .iter()
            .filter(|(_, item)| item.is_active())
            .map(|(id, item)| {
                (
                    id.to_string(),
                    Authority {
                        text: item.text.clone(),
                        origin: "user".into(),
                        sources: json!(item.introduced_by),
                    },
                )
            })
            .collect();
        items.insert(
            "goal".into(),
            Authority {
                text: projection.goal.text.clone(),
                origin: "user".into(),
                sources: json!(projection.goal.source_refs),
            },
        );
        let retired = snapshot
            .intents
            .iter()
            .filter(|(_, i)| !i.is_active())
            .map(|(id, _)| id.to_string())
            .collect();
        Ok(Self {
            goal: projection.goal.text,
            items,
            retired,
            permissions,
            conflicts: json!(projection.conflicts),
        })
    }
    pub fn permissions_for(&self, rule: &str) -> Result<BTreeMap<String, Authority>, String> {
        let mut result = BTreeMap::new();
        for id in self.permissions.get(rule).into_iter().flatten() {
            if self.retired.contains(id) {
                continue;
            }
            let item = self
                .items
                .get(id)
                .filter(|i| i.origin == "user")
                .ok_or_else(|| {
                    format!("MEASURE_PERMISSION_SOURCE stage=resolve: {rule} -> {id}")
                })?;
            result.insert(id.clone(), item.clone());
        }
        Ok(result)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub authority: String,
    pub unconditional_prohibition: bool,
    pub prohibition: bool,
    pub literal_check: bool,
    pub hunks: BTreeSet<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeFile {
    pub revision: String,
    pub text: String,
    pub digest: String,
}
impl CodeFile {
    pub fn new(revision: &str, text: &str) -> Self {
        Self {
            revision: revision.into(),
            text: text.into(),
            digest: digest(text.as_bytes()),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hunk {
    pub file: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Satisfied,
    Violated,
    Unclear,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    Behavior,
    NotViolated,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeQuote {
    pub file: String,
    pub first_line: usize,
    pub last_line: usize,
    pub text: String,
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub requirement: String,
    pub finding: String,
    pub status: Status,
    pub kind: FindingKind,
    pub quotes: Vec<CodeQuote>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub revision: String,
    pub baseline: Option<String>,
    pub checkpoint: bool,
    pub code_changed: bool,
    pub intent: CurrentIntent,
    pub impacted: BTreeSet<String>,
    pub rules: BTreeMap<String, Rule>,
    pub hunks: BTreeMap<String, Hunk>,
    pub files: BTreeMap<String, CodeFile>,
    pub matrix: Vec<Row>,
    pub carried: Value,
}
#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub revision: String,
    pub literal: Prepared,
    pub citations: Prepared,
    pub touched: BTreeSet<String>,
    pub carried: Value,
    pub matrix_pending: BTreeSet<String>,
    pub absence_rows: BTreeSet<String>,
    pub correction_signals: Vec<Value>,
    pub matrix_prompt: String,
}
