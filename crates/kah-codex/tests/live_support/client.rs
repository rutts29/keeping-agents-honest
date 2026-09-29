use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

pub struct Client {
    child: Child,
    input: Option<ChildStdin>,
    messages: Receiver<Value>,
    pub events: Vec<Value>,
    next: u64,
}

impl Client {
    pub fn start_command(mut command: Command, stderr: &Path) -> Self {
        let mut child = command
            .env_remove("HERDR_ENV")
            .env_remove("CODEX_THREAD_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(File::create(stderr).unwrap())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        let (tx, messages) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                let Ok(value) = serde_json::from_str(&line) else {
                    break;
                };
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            messages,
            events: vec![],
            next: 1,
        };
        client.request("initialize", json!({"clientInfo":{"name":"kah_live_journey","version":"0.1"},"capabilities":{"experimentalApi":true}}));
        client.send(json!({"method":"initialized"}));
        client
    }

    fn send(&mut self, value: Value) {
        writeln!(self.input.as_mut().unwrap(), "{value}").unwrap();
    }

    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({"id":id,"method":method,"params":params}));
        let response = self.wait(Duration::from_secs(45), |value| value["id"] == id);
        assert!(response.get("error").is_none(), "{method}: {response}");
        response["result"].clone()
    }

    pub fn wait(&mut self, timeout: Duration, matches: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            let value = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("native app-server closed or timed out");
            let found = matches(&value);
            self.events.push(value.clone());
            if found {
                return value;
            }
        }
    }

    pub fn turn(&mut self, id: &str, text: &str) -> Value {
        self.request(
            "turn/start",
            json!({"threadId":id,"input":[{"type":"text","text":text}]}),
        );
        self.wait(Duration::from_secs(180), |v| {
            v["method"] == "turn/completed" && v["params"]["threadId"] == id
        })["params"]["turn"]
            .clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
