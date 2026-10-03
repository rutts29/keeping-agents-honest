//! Host-authorized user input, interleaved with native intake without losing origin.
//! Verified quotes retain their native source; explicit host user relays carry
//! origin=relayed instead of claiming an independently verified original quote.
use crate::{
    intake::Input,
    session::{Binding, Store},
};
use kah_core::{
    intent_history::{SourceProvider, SourceRef, SourceRole},
    session_bundle::{BundleRecord, NativeGoal, NeutralSessionBundle},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Quote {
    pub receipt: String,
    pub source: SourceRef,
    pub text: String,
    pub dispatch_item: Option<kah_core::intent_history::IntentItem>,
}
impl Quote {
    pub fn validate(&self) -> Result<(), String> {
        self.source.validate().map_err(|e| e.to_string())?;
        let record = BundleRecord::new(&self.source.record, SourceRole::User, &self.text)
            .map_err(|e| e.to_string())?;
        if self.source.role != SourceRole::User
            || self.source.content_digest != record.content_digest()
            || self.receipt.is_empty()
            || self.receipt.len() > 128
        {
            return Err("INHERITED_SOURCE stage=intake: quote is not exact user source".into());
        }
        if self.dispatch_item.as_ref().is_some_and(|item| {
            item.text != self.text
                || item.id.as_str() != crate::verbatim::item_id(&self.source)
                || item.introduced_by != [self.source.clone()]
                || !item.is_active()
        }) {
            return Err("INHERITED_ITEM stage=intake: dispatch item differs from source".into());
        }
        Ok(())
    }
    pub fn key(&self) -> String {
        format!(
            "inherited:{:x}",
            Sha256::digest(serde_json::to_vec(&self.source).unwrap())
        )
    }
}

/// Preserve intent semantics and uncertainty, not the parent's completion evidence
/// or replacement graph. The assignment remains the worker's goal.
pub fn worker_item(quote: &Quote) -> Option<kah_core::intent_history::IntentItem> {
    use kah_core::intent_history::{IntentItem, IntentLifecycle};
    let parent = quote.dispatch_item.as_ref()?;
    let mut item = IntentItem::new(
        parent.id.clone(),
        parent.kind.clone(),
        &parent.text,
        parent.introduced_by.clone(),
    );
    item.review_required = parent.review_required
        || !parent.uncertain_supersedes.is_empty()
        || parent.lifecycle != IntentLifecycle::Active;
    item.kind_uncertain = parent.kind_uncertain;
    item.user_confirmed = parent.user_confirmed;
    item.changed_by = parent.changed_by.clone();
    Some(item)
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Inbox {
    pub provider: SourceProvider,
    pub quotes: Vec<Quote>,
}
impl Inbox {
    pub fn validate(&self) -> Result<(), String> {
        for quote in &self.quotes {
            quote.validate()?;
        }
        Ok(())
    }
    pub fn find(&self, key: &str) -> Option<&Quote> {
        self.quotes.iter().find(|q| q.key() == key)
    }
}

pub fn merge(
    binding: &Binding,
    store: &Store,
    native: Option<NeutralSessionBundle>,
) -> Result<Option<NeutralSessionBundle>, String> {
    let Some(inbox) = store.inherited()? else {
        return Ok(native);
    };
    let prior = store.sources()?;
    let mut records = native
        .as_ref()
        .map(|n| n.records().to_vec())
        .unwrap_or_default();
    for quote in &inbox.quotes {
        if records.iter().any(|r| r.id() == quote.key()) {
            return Err("INHERITED_COLLISION stage=intake".into());
        }
        records.push(
            BundleRecord::new(quote.key(), SourceRole::User, &quote.text)
                .map_err(|e| e.to_string())?,
        );
    }
    let records = stable_order(&prior, records)?;
    let digest = format!(
        "sha256:{:x}",
        Sha256::digest(binding.project.as_os_str().as_encoded_bytes())
    );
    if native
        .as_ref()
        .is_some_and(|n| n.source().provider() != inbox.provider)
    {
        return Err("INHERITED_PROVIDER stage=intake".into());
    }
    NeutralSessionBundle::from_records_with_native_goal(
        inbox.provider,
        &binding.session,
        digest,
        native
            .as_ref()
            .map(|n| n.native_goal().clone())
            .unwrap_or(NativeGoal::Unknown),
        records,
    )
    .map(Some)
    .map_err(|e| e.to_string())
}
fn stable_order(prior: &[Input], records: Vec<BundleRecord>) -> Result<Vec<BundleRecord>, String> {
    let mut by_id = records
        .iter()
        .map(|r| (r.id().to_owned(), r.clone()))
        .collect::<BTreeMap<_, _>>();
    if by_id.len() != records.len() {
        return Err("INHERITED_COLLISION stage=intake".into());
    }
    let mut merged = vec![];
    let old_native = prior
        .iter()
        .filter(|i| i.local_record.is_none())
        .map(Input::key)
        .collect::<Vec<_>>();
    let new_native = records
        .iter()
        .filter(|r| !r.id().starts_with("inherited:"))
        .map(BundleRecord::id)
        .collect::<Vec<_>>();
    if !new_native.starts_with(&old_native) {
        return Err("OBSERVER_SOURCE_REWRITE stage=intake".into());
    }
    for input in prior {
        let record = by_id
            .remove(input.key())
            .ok_or("OBSERVER_SOURCE_REWRITE stage=intake")?;
        if record.content() != input.text {
            return Err("OBSERVER_SOURCE_REWRITE stage=intake".into());
        }
        merged.push(record);
    }
    for record in records {
        if by_id.remove(record.id()).is_some() {
            merged.push(record);
        }
    }
    Ok(merged)
}
