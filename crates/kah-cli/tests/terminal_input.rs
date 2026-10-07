#[path = "terminal_support/input.rs"]
mod input;

#[test]
fn billing_screens_abort_before_writing_any_input() {
    // Native options/footer shapes: Codex 0.160.0 actionable_banner.rs and
    // Claude 2.1.292 shipped rate-limit-options module (see native_billing.rs).
    for screen in [
        "Usage limit reached\r\n› 1. Buy credits\r\n  2. Continue\r\nenter to confirm · esc to go back",
        "What do you want to do?\r\n❯ 1. Upgrade your plan\r\n  2. Stop and wait for limit to reset\r\nEnter to select · Esc to cancel",
        "Usage limit reached\r\n› 1. Buy \x1b[31mcredits\x1b[0m\r\nPress a number to choose",
    ] {
        for key in ["\r", "s", "\x1b[A", "\x1b[1;1R", "\x04", "task text"] {
            let mut written = Vec::new();
            assert!(input::send(screen.as_bytes(), key, &mut written).is_err());
            assert!(written.is_empty(), "input reached the terminal");
        }
    }
}

#[test]
fn prose_and_retired_billing_screens_preserve_input() {
    for screen in [
        "The Jev payment outage is resolved.\r\n❯ Ready\r\n? for shortcuts",
        "Switch model? Yes, switch to Sonnet",
        "Usage limit reached\r\n› 1. Buy credits\r\nPress a number to choose\x1b[2J\x1b[HReady",
    ] {
        let mut written = Vec::new();
        input::send(screen.as_bytes(), "\r", &mut written).unwrap();
        assert_eq!(written, b"\r");
    }
}
