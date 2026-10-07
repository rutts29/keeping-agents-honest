//! openai/codex rust-v0.160.0, app-server-protocol/src/protocol/v2/thread.rs:
//! ThreadLoadedListResponse is in-memory ownership, not persisted visibility.
use kah_codex::delivery::{self, Rpc, RpcError};
use serde_json::{Value, json};
struct Server {
    ids: Vec<String>,
    calls: Vec<String>,
}
impl Rpc for Server {
    fn request(&mut self, method: &str, _: Value) -> Result<Value, RpcError> {
        self.calls.push(method.into());
        assert_eq!(
            method, "thread/loaded/list",
            "ownership check must not read/steer another server's thread"
        );
        Ok(json!({"data":self.ids,"nextCursor":null}))
    }
}
#[test]
fn a_socket_does_not_prove_that_daemon_owns_an_embedded_thread() {
    let mut server = Server {
        ids: vec!["daemon-owned".into()],
        calls: vec![],
    };
    assert!(!delivery::owns_session(&mut server, "embedded").unwrap());
    assert!(delivery::owns_session(&mut server, "daemon-owned").unwrap());
    assert_eq!(
        server.calls,
        vec!["thread/loaded/list", "thread/loaded/list"]
    );
}
