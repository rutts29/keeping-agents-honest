//! Preserve the explicit safe Choice order on the wire, independent of maps.
use super::*;
use serde::ser::{SerializeMap, SerializeStruct};
#[derive(Clone, Debug)]
pub struct Prepared {
    pub state: Value,
    pub questions: BTreeMap<String, Query>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Query {
    Noul {
        instructions: String,
    },
    Choice {
        instructions: String,
        criteria: Vec<(String, String)>,
    },
}
impl Prepared {
    pub fn body(&self) -> Result<String, String> {
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            state: &'a Value,
            questions: WireQuestions<'a>,
        }
        serde_json::to_string(&Body {
            model: "jev-1.13.0",
            state: &self.state,
            questions: WireQuestions(&self.questions),
        })
        .map_err(|e| format!("MEASURE_SERIALIZE stage=request: {e}"))
    }
    /// Decompose a literal rule into complete hunk questions, never text slices.
    /// Used when the real provider requires more requests; no size cap here.
    pub fn whole_hunks(&self) -> Vec<Self> {
        let mut results = vec![];
        for (id, query) in &self.questions {
            let Some((_, old_key)) = id.split_once('.') else {
                continue;
            };
            let Query::Noul { instructions } = query else {
                continue;
            };
            let context = &self.state["checks"][old_key];
            for (hunk_id, hunk) in context["diff_hunks"]
                .as_object()
                .into_iter()
                .flat_map(|m| m.iter())
            {
                let name = key(&format!("{old_key}:{hunk_id}"));
                let mut local = context.clone();
                local["diff_hunks"] = json!({hunk_id:hunk});
                results.push(Self {
                    state: json!({"checks":{&name:local}}),
                    questions: BTreeMap::from([(
                        format!("rule.{name}"),
                        Query::Noul {
                            instructions: instructions
                                .replace(&format!("checks.{old_key}"), &format!("checks.{name}")),
                        },
                    )]),
                });
            }
        }
        results
    }
    pub fn separate(&self) -> Vec<Self> {
        self.questions
            .iter()
            .map(|(id, q)| {
                let key = id.split_once('.').map(|(_, key)| key).unwrap_or(id);
                let mut state = json!({});
                for group in ["checks", "rows"] {
                    if let Some(value) = self.state[group].get(key) {
                        state[group] = json!({key:value});
                    }
                }
                Self {
                    state,
                    questions: BTreeMap::from([(id.clone(), q.clone())]),
                }
            })
            .collect()
    }
}
impl Serialize for Prepared {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_struct("Prepared", 2)?;
        map.serialize_field("state", &self.state)?;
        map.serialize_field("questions", &WireQuestions(&self.questions))?;
        map.end()
    }
}
struct WireQuestions<'a>(&'a BTreeMap<String, Query>);
impl Serialize for WireQuestions<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (id, query) in self.0 {
            map.serialize_entry(id, &WireQuery(query))?;
        }
        map.end()
    }
}
struct WireQuery<'a>(&'a Query);
impl Serialize for WireQuery<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        match self.0 {
            Query::Noul { instructions } => {
                map.serialize_entry("type", "noul")?;
                map.serialize_entry("instructions", instructions)?;
            }
            Query::Choice {
                instructions,
                criteria,
            } => {
                map.serialize_entry("type", "choice")?;
                map.serialize_entry("instructions", instructions)?;
                map.serialize_entry("criteria", &WireCriteria(criteria))?;
            }
        }
        map.end()
    }
}
struct WireCriteria<'a>(&'a [(String, String)]);
impl Serialize for WireCriteria<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (label, meaning) in self.0 {
            map.serialize_entry(label, meaning)?;
        }
        map.end()
    }
}
