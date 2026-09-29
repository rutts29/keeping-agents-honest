//! Explicit in-session control grammar for the Codex plugin.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginControl {
    On,
    Off,
    Status,
}

pub fn parse(prompt: &str) -> Option<PluginControl> {
    let mut fields = prompt.split_whitespace();
    if !matches!(fields.next()?, "$driftctl" | "$driftctl-codex:driftctl") {
        return None;
    }
    let action = match fields.next()? {
        "on" => PluginControl::On,
        "off" => PluginControl::Off,
        "status" => PluginControl::Status,
        _ => return None,
    };
    fields.next().is_none().then_some(action)
}

#[cfg(test)]
mod tests;
