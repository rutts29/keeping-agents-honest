use kah_codex::control::Control;
use serde_json::{Value, json};
use std::{fs, os::unix::net::UnixListener, thread, time::Duration};

#[test]
fn daemon_transport_uses_websocket_framing_on_a_local_socket() {
    let path = std::env::temp_dir().join(format!("kah-daemon-control-{}.sock", std::process::id()));
    let listener = UnixListener::bind(&path).unwrap();
    let server = thread::spawn(move || {
        let mut socket = tungstenite::accept(listener.accept().unwrap().0).unwrap();
        let initialize: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(initialize["method"], "initialize");
        socket
            .send(tungstenite::Message::Text(
                json!({"id":initialize["id"],"result":{}})
                    .to_string()
                    .into(),
            ))
            .unwrap();
        let initialized: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(initialized["method"], "initialized");
        let request: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        socket
            .send(tungstenite::Message::Text(
                json!({"id":request["id"],"result":{"confirmed":true}})
                    .to_string()
                    .into(),
            ))
            .unwrap();
    });
    let mut control = Control::connect_daemon(&path, Duration::from_secs(2)).unwrap();
    assert_eq!(
        control.request("fixture/read", json!({})).unwrap()["confirmed"],
        true
    );
    drop(control);
    server.join().unwrap();
    fs::remove_file(path).unwrap();
}
