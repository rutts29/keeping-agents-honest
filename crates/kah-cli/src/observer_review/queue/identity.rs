//! Source identity plus exact content, never text matching or turn identity.
use super::*;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) fn steering(trigger: &Value) -> Result<Option<String>> {
    let Some(value) = trigger.get("uncertain_user_input") else {
        return Ok(None);
    };
    let Ok(source) = serde_json::from_value::<kah_observer::intake::Input>(value.clone()) else {
        return Ok(None);
    };
    let value = json!({"record":source.key(),"source":source.source,"text":source.text});
    Ok(Some(format!(
        "steering-{:x}.json",
        Sha256::digest(serde_json::to_vec(&value)?)
    )))
}

pub(super) fn retained(root: &Path, name: &str, trigger: &Value) -> Result<bool> {
    if !name.starts_with("steering-") {
        return Ok(false);
    }
    for directory in ["queued", "covered-triggers", "queue-retired"] {
        if let Some(saved) = super::super::state::read::<Wake>(&root.join(directory).join(name))? {
            if saved.purpose != Purpose::Steering || steering(&saved.trigger)? != steering(trigger)?
            {
                return Err("REVIEW_STEERING_IDENTITY_CHANGED stage=queue".into());
            }
            return Ok(true);
        }
    }
    Ok(false)
}
