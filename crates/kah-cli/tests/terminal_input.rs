#[path = "terminal_support/input.rs"]
mod input;

#[test]
fn billing_screens_abort_before_writing_any_input() {
    for screen in [
        "BILLING details",
        "Buy usage credits",
        "Confirm purchase",
        "Upgrade now",
        "Payment required",
        "Pay\r\nment required",
        "Buy credits\x1b[2J\x1b[HReady",
        "\x1b[2J\x1b[HCRED\x1b[31mITS",
        "\x1b[2J\x1b[HPaym nt\x1b[1;5He",
    ] {
        for key in ["\r", "s", "\x1b[A", "\x1b[1;1R", "\x04", "task text"] {
            let mut written = Vec::new();
            let result = input::send(screen.as_bytes(), key, &mut written);
            assert!(result.is_err(), "billing screen accepted input");
            assert!(written.is_empty(), "input reached the terminal");
        }
    }
}

#[test]
fn ordinary_model_confirmation_preserves_input() {
    let mut written = Vec::new();
    input::send(b"Switch model? Yes, switch to Sonnet", "\r", &mut written).unwrap();
    assert_eq!(written, b"\r");
}
