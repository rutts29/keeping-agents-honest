use super::{PluginControl, parse};

#[test]
fn accepts_only_the_explicit_two_token_control_grammar() {
    assert_eq!(parse("$driftctl on"), Some(PluginControl::On));
    assert_eq!(parse("  $driftctl\tstatus\n"), Some(PluginControl::Status));
    assert_eq!(parse("$driftctl off"), Some(PluginControl::Off));
    assert_eq!(
        parse("$driftctl-codex:driftctl on"),
        Some(PluginControl::On)
    );
    assert_eq!(
        parse("$driftctl-codex:driftctl status"),
        Some(PluginControl::Status)
    );
    assert_eq!(
        parse("$driftctl-codex:driftctl off"),
        Some(PluginControl::Off)
    );

    for prompt in [
        "$driftctl",
        "$driftctl ON",
        "$driftctl on now",
        "please use $driftctl on",
        "`$driftctl on`",
        "$driftctl-on",
        "$other:driftctl on",
        "$driftctl-codex:driftctl on now",
    ] {
        assert_eq!(parse(prompt), None, "unexpected control match: {prompt}");
    }
}
