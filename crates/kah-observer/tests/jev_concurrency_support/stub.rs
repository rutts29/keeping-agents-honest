use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
pub struct Stub {
    pub address: SocketAddr,
    attempts: Arc<Trace>,
    panics: Arc<AtomicUsize>,
}
impl Stub {
    pub fn start(corpus: &Corpus, lower: bool) -> Self {
        Self::configured(corpus, lower, None)
    }
    pub fn outage(corpus: &Corpus, status: u16) -> Self {
        Self::configured(corpus, false, Some(status))
    }
    fn configured(corpus: &Corpus, lower: bool, outage: Option<u16>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let replies = Arc::new(corpus.replies.clone());
        let requests = Arc::new(
            corpus
                .requests
                .iter()
                .map(|r| (r.id.clone(), r.body.clone()))
                .collect::<BTreeMap<_, _>>(),
        );
        let attempts = Arc::new(Mutex::new(vec![]));
        let trace = attempts.clone();
        let panics = Arc::new(AtomicUsize::new(0));
        let failures = panics.clone();
        let window = Arc::new(Mutex::new(Vec::<(Instant, u64)>::new()));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let stream = stream.unwrap();
                let replies = replies.clone();
                let requests = requests.clone();
                let trace = trace.clone();
                let window = window.clone();
                let failures = failures.clone();
                std::thread::spawn(move || {
                    if std::panic::catch_unwind(|| {
                        serve(stream, &replies, &requests, &trace, &window, lower, outage)
                    })
                    .is_err()
                    {
                        failures.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });
        Self {
            address,
            attempts,
            panics,
        }
    }
    pub fn assert_retry_after_respected(&self) {
        let attempts = self.attempts.lock().unwrap();
        let mut waits = BTreeMap::new();
        for (id, at, limited, delay) in attempts.iter() {
            if let Some((before, wait)) = waits.remove(id) {
                assert!(at.duration_since(before) >= Duration::from_millis(wait));
            }
            if *limited {
                waits.insert(id.clone(), (*at, *delay));
            }
        }
        assert!(attempts.iter().any(|(_, _, limited, _)| *limited));
    }
    pub fn counts(&self) -> BTreeMap<String, usize> {
        assert_eq!(
            self.panics.load(Ordering::Relaxed),
            0,
            "replay handler panicked"
        );
        let mut counts = BTreeMap::new();
        for (id, _, _, _) in self.attempts.lock().unwrap().iter() {
            *counts.entry(id.clone()).or_default() += 1;
        }
        counts
    }
}
type Replies = BTreeMap<String, (u16, String, u64, u64)>;
type Trace = Mutex<Vec<(String, Instant, bool, u64)>>;
fn serve(
    stream: TcpStream,
    replies: &Replies,
    requests: &BTreeMap<String, String>,
    trace: &Trace,
    window: &Mutex<Vec<(Instant, u64)>>,
    lower: bool,
    outage: Option<u16>,
) {
    let Some((reader, headers, wire)) = incoming(stream) else {
        return;
    };
    let id = &headers["x-kah-request-id"];
    assert_eq!(&wire, &requests[id]);
    let (status, body, tokens, latency) = &replies[id];
    let now = Instant::now();
    let outage = outage.filter(|_| {
        id == "phase1-1" && !trace.lock().unwrap().iter().any(|(key, _, _, _)| key == id)
    });
    let mut active = window.lock().unwrap();
    active.retain(|(at, _)| now.duration_since(*at) < Duration::from_secs(1));
    let forced = lower
        && matches!(id.as_str(), "phase1-0" | "phase1-1")
        && !trace.lock().unwrap().iter().any(|(key, _, _, _)| key == id);
    let limited = lower
        && (forced
            || active.len() >= 40
            || active.iter().map(|(_, n)| n).sum::<u64>() + tokens > 50_000);
    if !limited {
        active.push((now, *tokens));
    }
    drop(active);
    let delay = if id == "phase1-0" { 1000 } else { 125 };
    trace
        .lock()
        .unwrap()
        .push((id.clone(), now, limited, delay));
    let (status, body, extra) = if let Some(status) = outage {
        (
            status,
            "{\"error\":\"service unavailable\"}".into(),
            String::new(),
        )
    } else if limited {
        let retry = if id == "phase1-0" {
            "retry-after: 1\r\n".into()
        } else {
            format!("retry-after-ms: {delay}\r\n")
        };
        (
            if id == "phase1-1" { 529 } else { 429 },
            "{\"error\":\"rate limit\"}".to_owned(),
            format!(
                "{retry}x-ratelimit-limit-requests: 40\r\nx-ratelimit-limit-tokens: 50000\r\nx-ratelimit-window-seconds: 1\r\n"
            ),
        )
    } else {
        std::thread::sleep(Duration::from_millis(*latency));
        (*status, body.clone(), String::new())
    };
    let mut stream = reader.into_inner();
    // Killing the fixture client can close its socket before this retained reply.
    let _ = write!(
        stream,
        "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{extra}\r\n{body}",
        body.len()
    );
}
fn incoming(stream: TcpStream) -> Option<(BufReader<TcpStream>, BTreeMap<String, String>, String)> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut headers = BTreeMap::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if line == "\r\n" {
            break;
        }
        let (key, value) = line.trim_end().split_once(':').unwrap();
        headers.insert(key.to_ascii_lowercase(), value.trim().to_string());
    }
    let size: usize = headers["content-length"].parse().unwrap();
    let mut body = vec![0; size];
    reader.read_exact(&mut body).ok()?;
    Some((reader, headers, String::from_utf8(body).unwrap()))
}
