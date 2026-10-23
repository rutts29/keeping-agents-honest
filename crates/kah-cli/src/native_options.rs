//! How Codex command lines spell options that take a value. Every scanner of
//! native arguments reads this one table, so they agree on which token is an
//! option's value and which is a subcommand or prompt.

/// Long options that take a value (`codex --help`).
const VALUE_OPTIONS: &[&str] = &[
    "config",
    "enable",
    "disable",
    "remote",
    "remote-auth-token-env",
    "image",
    "local-provider",
    "model",
    "profile",
    "sandbox",
    "cd",
    "add-dir",
    "ask-for-approval",
];

/// Short options that take a value: -c -i -m -p -s -C -a.
const SHORT_VALUE_OPTIONS: &[char] = &['c', 'i', 'm', 'p', 's', 'C', 'a'];

/// Where an argument keeps its option value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Take {
    /// The value is the next argument (`--profile work`, `-p work`).
    Next,
    /// The value is part of the argument (`--profile=work`, `-pwork`, `-p=work`).
    Inline,
    /// Not an option with a value.
    No,
}

pub fn takes(arg: &str) -> Take {
    if let Some(long) = arg.strip_prefix("--") {
        let (name, inline) = match long.split_once('=') {
            Some((name, _)) => (name, true),
            None => (long, false),
        };
        return match (VALUE_OPTIONS.contains(&name), inline) {
            (true, true) => Take::Inline,
            (true, false) => Take::Next,
            _ => Take::No,
        };
    }
    let mut chars = arg.strip_prefix('-').unwrap_or_default().chars();
    match (chars.next(), chars.as_str()) {
        (Some(short), "") if SHORT_VALUE_OPTIONS.contains(&short) => Take::Next,
        (Some(short), _) if SHORT_VALUE_OPTIONS.contains(&short) => Take::Inline,
        _ => Take::No,
    }
}

/// The model of an attached spelling: `--model=V`, `-m=V` or `-mV`.
pub fn inline_model(arg: &str) -> Option<&str> {
    if let Some(value) = arg.strip_prefix("--model=") {
        return Some(value);
    }
    let rest = arg.strip_prefix("-m").filter(|rest| !rest.is_empty())?;
    Some(rest.strip_prefix('=').unwrap_or(rest))
}

/// Whether the next argument is the model: `--model V` or `-m V`.
pub fn separate_model(arg: &str) -> bool {
    matches!(arg, "--model" | "-m")
}
