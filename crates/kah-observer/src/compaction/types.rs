use super::*;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ResolverUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolverMetadata {
    pub model: String,
    pub reasoning: String,
    pub calls: u8,
    pub elapsed_ms: u128,
    pub usage: ResolverUsage,
    pub usage_complete: bool,
    pub prompt_schema_version: u32,
    pub proposal_schema_version: u32,
    pub last_validation_failure: Option<String>,
    pub artifact_ids: Vec<String>,
}

pub(super) struct CodexCall {
    pub final_message: String,
    pub usage: ResolverUsage,
    pub artifact_id: String,
}
