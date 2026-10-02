//! Receipt matching happens at a primary native hook, never in model inference.
use crate::{Result, relay::Receipt, relay_store};
use kah_core::run_store::RunStore;
use kah_observer::session::{Binding, Store};
use serde_json::{Value, json};

pub fn handle(binding: &Binding, input: &Value, provider: &str) -> Result<()> {
    relay_store::register(binding, input, provider)?;
    if input["hook_event_name"] != "UserPromptSubmit" {
        return Ok(());
    }
    let Some(prompt) = input["prompt"].as_str() else {
        return Ok(());
    };
    let prompt = if provider == "claude" {
        if kah_claude::paste::body(prompt).is_none()
            && kah_claude::paste::content_start(prompt) != prompt
            && ["[kah-relay]", "[kah-user]"]
                .iter()
                .any(|prefix| kah_claude::paste::content_start(prompt).starts_with(prefix))
        {
            return Err("RELAY_NATIVE_FRAME stage=receive".into());
        }
        kah_claude::paste::body(prompt).unwrap_or(prompt)
    } else {
        prompt
    };
    if receive_user(binding, prompt)? {
        return Ok(());
    }
    let Some(first) = prompt
        .lines()
        .next()
        .and_then(|s| s.strip_prefix("[kah-relay] receipt="))
    else {
        return Ok(());
    };
    let directory = relay_store::directory(&binding.session)?;
    let receipt: Receipt = relay_store::read(&directory.join(relay_store::receipt_name(first)?))?;
    let message = receipt.message();
    if receipt.id != first
        || receipt.receiver != binding.session
        || receipt.quote.receipt != receipt.id
        || receipt.quote.source.session_private() != receipt.sender
        || receipt.quote.dispatch_item.is_some()
        || prompt != message && prompt.strip_suffix('\n') != Some(message.as_str())
    {
        return Err("RELAY_RECEIPT_MISMATCH stage=receive".into());
    }
    receipt.quote.validate()?;
    let endpoint = relay_store::endpoint(&binding.session)?;
    Store::accept_inherited(
        &RunStore::default_state_root()?,
        &binding.session,
        endpoint.source_provider()?,
        receipt.quote.clone(),
    )?;
    crate::unattended_report::save(
        &directory,
        &format!("received-{}.json", receipt.id),
        &json!({"receipt":receipt.id,"receiver":binding.session,"source":receipt.quote.source,"paraphraseRole":"delegate","paraphraseSupported":receipt.supported}),
    )?;
    Ok(())
}

fn receive_user(binding: &Binding, prompt: &str) -> Result<bool> {
    let Some(message) = kah_observer::relayed_user::Message::parse(prompt)? else {
        return Ok(false);
    };
    let endpoint = relay_store::endpoint(&binding.session)?;
    message.accept(
        &RunStore::default_state_root()?,
        &binding.session,
        endpoint.source_provider()?,
    )?;
    Ok(true)
}
