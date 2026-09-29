use super::*;

pub(super) struct OperationState<'a> {
    pub(super) history: History,
    pub(super) lookup: BTreeMap<&'a str, (&'a BundleRecord, &'a SourceRef)>,
    pub(super) digest: &'a str,
    pub(super) referenced: BTreeSet<String>,
    pub(super) keys: BTreeSet<String>,
    pub(super) intents: BTreeMap<String, IntentId>,
}
