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
    /// Split by whole source hunks/files, retaining every applicable rule.
    /// Used only for lossless request construction, never as a byte cap.
    pub fn whole_hunks(&self) -> Vec<Self> {
        self.state["diff_hunks"]
            .as_object()
            .into_iter()
            .flat_map(|hunks| hunks.iter())
            .map(|(id, hunk)| {
                let mut state = self.state.clone();
                state["diff_hunks"] = json!({id:hunk});
                if let Some(checks) = state["checks"].as_object_mut() {
                    for rule in checks.values_mut() {
                        let applies = rule["hunk_ids"]
                            .as_array()
                            .is_some_and(|ids| ids.iter().any(|old| old == id));
                        rule["hunk_ids"] = if applies { json!([id]) } else { json!([]) };
                    }
                }
                Self {
                    state,
                    questions: self.questions.clone(),
                }
            })
            .collect()
    }
    pub fn separate(&self) -> Vec<Self> {
        self.questions
            .iter()
            .map(|(id, q)| {
                let key = id.split_once('.').map(|(_, key)| key).unwrap_or(id);
                let mut state = json!({});
                for shared in ["diff_hunks", "current_permissions"] {
                    if let Some(value) = self.state.get(shared) {
                        state[shared] = value.clone();
                    }
                }
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
