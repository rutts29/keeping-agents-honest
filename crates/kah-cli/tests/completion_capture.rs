use kah_cli::completion_capture::{self, Request};
use kah_observer::judge::{self, Questions};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

#[test]
fn only_scrubbed_actual_requests_are_captured_privately() {
    let token = concat!("sk-", "test-abcdefghijklmnopqrstuvwxyz0123456789");
    let questions = Questions::from([(
        "proof".into(),
        judge::Question::Noul {
            instructions: format!("Does the code avoid printing {token}?"),
        },
    )]);
    let input = json!({"api_key":"secret-value","code":format!("const password = \"hunter2\";\nconst answer = 42;\n// {token}"),"unchanged":"module.exports = n => n * 2;"});
    let request = Request::new("jev-latest", &input, &questions).unwrap();
    let body = request.body();
    assert_eq!(
        Request::new(request.model(), request.state(), request.questions())
            .unwrap()
            .body(),
        body
    );
    for secret in [token, "hunter2", "secret-value"] {
        assert!(!body.to_string().contains(secret));
    }
    assert!(body.to_string().contains("const answer = 42"));
    assert_eq!(body["state"]["unchanged"], input["unchanged"]);
    let root = std::env::temp_dir().join(format!("kah-completion-capture-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let result = Err(judge::Error::new("TEST_FAILURE"));
    let path = completion_capture::save(&root, "run-fixture", "abc", "artifact", &request, &result)
        .unwrap();
    let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["request"], body);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(saved["redactions"].as_u64().unwrap() >= 3);
    let mut tampered = saved.clone();
    tampered["request"]["state"]["unchanged"] = "tampered".into();
    fs::write(&path, tampered.to_string()).unwrap();
    assert!(
        kah_cli::completion_replay::run(&root, &path)
            .unwrap_err()
            .starts_with("COMPLETION_REPLAY_INTEGRITY")
    );
    let archive = root.join(".local/share/kah-eval");
    fs::remove_dir_all(&archive).unwrap();
    fs::create_dir(root.join("public")).unwrap();
    symlink(root.join("public"), archive).unwrap();
    assert!(
        completion_capture::save(&root, "run-fixture", "abc", "artifact", &request, &result)
            .is_err()
    );
    assert_eq!(fs::read_dir(root.join("public")).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn redaction_preserves_requirements_and_catches_multiple_assignments() {
    let questions = Questions::from([(
        "proof".into(),
        judge::Question::Noul {
            instructions: "Implement a Bearer token parser; preserve the public API.".into(),
        },
    )]);
    let request = Request::new("jev-latest", &json!({"text":"password='hunter2'; api_key=secret42; keep_this();", "nested":"{\"password\":\"inside-json\"}","pem":concat!("before\n-----BEGIN ", "RSA PRIVATE KEY-----\nprivate-material\n-----END RSA PRIVATE KEY-----\nafter")}), &questions).unwrap();
    let body = request.body().to_string();
    for secret in ["hunter2", "secret42", "inside-json", "private-material"] {
        assert!(!body.contains(secret), "{secret}");
    }
    assert!(body.contains("keep_this"));
    assert!(body.contains("before") && body.contains("after"));
    assert_eq!(
        serde_json::to_value(request.questions()).unwrap(),
        serde_json::to_value(questions).unwrap()
    );
}
