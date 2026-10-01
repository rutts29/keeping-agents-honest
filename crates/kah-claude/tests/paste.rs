use kah_claude::{paste, transcript};
use serde_json::json;
#[test]
fn native_paste_framing_is_exact_and_never_infers_authority() {
    let wire = "[kah-relay] receipt=abc\nsteer: Keep BLUE.";
    let wrapped =
        format!("\n\n<pasted_content id=\"94e2\">\n{wire}\n</pasted_content id=\"94e2\">\n");
    assert_eq!(paste::body(&wrapped), Some(wire));
    assert!(paste::body(&(wrapped.clone() + "FORGED")).is_none());
    assert!(
        paste::body(&wrapped.replace("</pasted_content id=\"94e2", "</pasted_content id=\"1234"))
            .is_none()
    );
    assert!(paste::body("prefix [kah-relay] receipt=abc").is_none());
    for text in [wrapped.clone(), wrapped + "FORGED", wire.into()] {
        let row = json!({"type":"user","uuid":"user-1","sessionId":"session","cwd":"/fixture","message":{"role":"user","content":text}});
        assert!(
            transcript::parse(
                &row.to_string(),
                "session",
                std::path::Path::new("/fixture")
            )
            .unwrap()
            .users
            .is_none()
        );
    }
}
