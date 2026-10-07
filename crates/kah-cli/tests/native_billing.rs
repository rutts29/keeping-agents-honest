use kah_cli::observer_review::delivery::safe_keyboard;

// Native rendering: openai/codex 0.160.0, tui/src/bottom_pane/actionable_banner.rs
// and backend_banner_personal_limit.snap; Claude 2.1.292 rate-limit-options
// uses _e(title="What do you want to do?") + Ue(options), shipped in its binary.
#[test]
fn native_billing_options_abort_but_transcript_prose_does_not() {
    for screen in [
        "Usage limit reached\nYour included usage is exhausted. Choose an option below to continue.\n› 1. Continue\n  2. Learn more\nenter to confirm · esc to go back",
        "Confirm amount\nSet your monthly spend limit to $150.\nType yes to confirm.\n│ yes │\nEsc to cancel",
        "Buy usage credits\nPayment: Visa\n│ 75 │\nBy confirming, you allow Anthropic to charge your card in the amount above.\nEsc to cancel",
        "Usage limit reached\nYour included usage is depleted.\n› 1. Open usage settings\n  2. View usage\nPress a number to choose · esc to dismiss · type to continue\n› Ask Codex to do anything\n? for shortcuts",
        "Usage limit reached\n› 1. Buy credits\n  2. Continue\nenter to confirm · esc to go back",
        "What do you want to do?\n❯ 1. Upgrade your plan\n  2. Stop and wait for limit to reset\nEnter to select · Esc to cancel",
        "Your Claude Code trial has ended.\nWhat do you want to do?\n❯ 1. Upgrade to Max\n  2. Add funds to continue with usage credits\nEnter to select · Esc to cancel",
    ] {
        assert!(safe_keyboard(screen).is_err(), "accepted modal: {screen}");
    }
    for screen in [
        "The Jev payment outage is resolved.\n› kah review request review-1\n? for shortcuts",
        "billing credit purchase upgrade payment\n❯ Ready\n? for shortcuts",
        "An old dialog said:\nWhat do you want to do?\n❯ 1. Upgrade your plan\nEnter to select · Esc to cancel\nThe payment outage is resolved.\n❯ kah review request review-1\n? for shortcuts",
        "payment issue in history\nSwitch model?\n❯ 1. Yes, switch to Sonnet\n  2. No, go back\nEnter to select · Esc to cancel",
    ] {
        assert!(safe_keyboard(screen).is_ok(), "blocked prose: {screen}");
    }
}
