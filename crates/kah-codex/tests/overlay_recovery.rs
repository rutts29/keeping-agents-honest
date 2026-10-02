use kah_codex::overlay::{Policy, prepare};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::PathBuf,
};
struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let f =
            Self(std::env::temp_dir().join(format!("kah-recovery-{}-{name}", std::process::id())));
        fs::create_dir_all(f.0.join("global")).unwrap();
        fs::write(f.0.join("global/config.toml"), "model='fixture'").unwrap();
        f.prepare().unwrap();
        f
    }
    fn prepare(&self) -> Result<(), kah_codex::overlay::OverlayError> {
        prepare(
            &self.0.join("global"),
            &self.0.join("overlay"),
            &Policy {
                restricted: true,
                online: false,
                root: &self.0,
                cwd: &self.0,
            },
        )
    }
    fn fork(&self, name: &str, global: &str, local: &str) {
        fs::write(self.0.join("global").join(name), global).unwrap();
        let path = self.0.join("overlay").join(name);
        fs::remove_file(&path).unwrap();
        fs::write(path, local).unwrap();
    }
    fn bytes(&self, side: &str, name: &str) -> Vec<u8> {
        fs::read(self.0.join(side).join(name)).unwrap()
    }
    fn linked(&self, name: &str) {
        assert_eq!(
            fs::read_link(self.0.join("overlay").join(name)).unwrap(),
            self.0.join("global").join(name)
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn rows(values: &[Value]) -> String {
    values.iter().map(|v| format!("{v}\n")).collect()
}
fn index(id: &str, name: &str, date: &str) -> Value {
    json!({"id":id,"thread_name":name,"updated_at":date})
}
fn auth(date: &str, token: &str, account: &str) -> String {
    json!({"auth_mode":"chatgpt","last_refresh":date,"tokens":{"account_id":account,"id_token":token,"access_token":token,"refresh_token":token}}).to_string()
}
#[test]
fn index_merges_by_id_and_absolute_timestamp_not_file_mtime() {
    let f = Fixture::new("index");
    f.fork(
        "session_index.jsonl",
        &rows(&[
            index("a", "global newest", "2026-10-02T10:00:00Z"),
            index("b", "old", "2026-10-01T00:00:00Z"),
        ]),
        &rows(&[
            index("a", "local old", "2026-10-02T10:30:00+01:00"),
            index("b", "local newest", "2026-10-02T00:00:00Z"),
            index("c", "local only", "2026-10-02T00:00:00Z"),
        ]),
    );
    f.prepare().unwrap();
    f.linked("session_index.jsonl");
    let bytes = f.bytes("global", "session_index.jsonl");
    let values = String::from_utf8(bytes.clone())
        .unwrap()
        .lines()
        .map(|r| serde_json::from_str::<Value>(r).unwrap())
        .collect::<Vec<_>>();
    for (id, name) in [
        ("a", "global newest"),
        ("b", "local newest"),
        ("c", "local only"),
    ] {
        assert_eq!(
            values.iter().rev().find(|v| v["id"] == id).unwrap()["thread_name"],
            name
        );
    }
    f.prepare().unwrap();
    assert_eq!(bytes, f.bytes("global", "session_index.jsonl"));
}
#[test]
fn history_appends_missing_occurrences_without_replacing_the_native_locked_inode() {
    let f = Fixture::new("history");
    let a = json!({"session_id":"a","ts":1,"text":"same prompt"});
    let b = json!({"session_id":"b","ts":2,"text":"new prompt"});
    f.fork(
        "history.jsonl",
        &rows(std::slice::from_ref(&a)),
        &rows(&[a.clone(), a.clone(), b]),
    );
    let inode = fs::metadata(f.0.join("global/history.jsonl"))
        .unwrap()
        .ino();
    f.prepare().unwrap();
    f.linked("history.jsonl");
    assert_eq!(
        inode,
        fs::metadata(f.0.join("global/history.jsonl"))
            .unwrap()
            .ino()
    );
    let bytes = f.bytes("global", "history.jsonl");
    assert_eq!(String::from_utf8(bytes.clone()).unwrap().lines().count(), 3);
    f.prepare().unwrap();
    assert_eq!(bytes, f.bytes("global", "history.jsonl"));
}
#[test]
fn auth_selects_the_whole_newer_refresh_atomically_in_either_direction() {
    for (suffix, global_time, local_time, expected) in [
        (
            "new-local",
            "2026-10-01T00:00:00Z",
            "2026-10-02T00:00:00Z",
            "local",
        ),
        (
            "new-global",
            "2026-10-02T00:00:00Z",
            "2026-10-01T00:00:00Z",
            "global",
        ),
    ] {
        let f = Fixture::new(suffix);
        f.fork(
            "auth.json",
            &auth(global_time, "FAKE_GLOBAL", "fixture"),
            &auth(local_time, "FAKE_LOCAL", "fixture"),
        );
        let selected = f.bytes(
            if expected == "local" {
                "overlay"
            } else {
                "global"
            },
            "auth.json",
        );
        f.prepare().unwrap();
        f.linked("auth.json");
        assert_eq!(selected, f.bytes("global", "auth.json"));
        if expected == "local" {
            assert_eq!(
                fs::metadata(f.0.join("global/auth.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        // 0.160.0 FileAuthStorage uses truncate/write, which follows the restored link.
        fs::write(
            f.0.join("overlay/auth.json"),
            auth("2026-10-03T00:00:00Z", "FAKE_REFRESH", "fixture"),
        )
        .unwrap();
        f.linked("auth.json");
        assert_eq!(
            f.bytes("overlay", "auth.json"),
            f.bytes("global", "auth.json")
        );
    }
}
#[test]
fn ambiguous_auth_never_overwrites_either_store_or_prints_credentials() {
    for (label, time, account) in [
        ("tie", "2026-10-01T00:00:00Z", "fixture"),
        ("account", "2026-10-02T00:00:00Z", "other"),
    ] {
        let f = Fixture::new(label);
        f.fork(
            "auth.json",
            &auth("2026-10-01T00:00:00Z", "FAKE_GLOBAL", "fixture"),
            &auth(time, "FAKE_LOCAL", account),
        );
        let before = [
            f.bytes("global", "auth.json"),
            f.bytes("overlay", "auth.json"),
        ];
        let error = f.prepare().unwrap_err();
        assert_eq!(error.code, "OVERLAY_SHARED_RECOVERY");
        assert!(error.to_string().contains("auth.json"));
        assert!(!error.to_string().contains("FAKE_"));
        assert_eq!(
            before,
            [
                f.bytes("global", "auth.json"),
                f.bytes("overlay", "auth.json")
            ]
        );
    }
}
#[test]
fn malformed_or_busy_history_is_retained_and_reports_its_path() {
    let f = Fixture::new("partial");
    f.fork("history.jsonl", "", "{\"text\":\"PRIVATE_FIXTURE");
    let error = f.prepare().unwrap_err().to_string();
    assert!(error.contains("history.jsonl") && !error.contains("PRIVATE_FIXTURE"));
    assert_eq!(f.bytes("global", "history.jsonl"), b"");
    fs::write(f.0.join("overlay/history.jsonl"), "").unwrap();
    let locked = fs::File::open(f.0.join("global/history.jsonl")).unwrap();
    locked.lock().unwrap();
    assert!(f.prepare().unwrap_err().to_string().contains("busy"));
    drop(locked);
    f.prepare().unwrap();
    f.linked("history.jsonl");
}
#[test]
fn cache_is_disposable_but_foreign_links_and_shared_directories_are_not() {
    let f = Fixture::new("cache");
    f.fork("models_cache.json", "canonical cache", "forked cache");
    f.prepare().unwrap();
    f.linked("models_cache.json");
    assert_eq!(f.bytes("global", "models_cache.json"), b"canonical cache");
    let target = f.0.join("overlay/session_index.jsonl");
    fs::remove_file(&target).unwrap();
    symlink(f.0.join("foreign"), &target).unwrap();
    assert_eq!(f.prepare().unwrap_err().code, "OVERLAY_SHARED_PATH");
    fs::remove_file(target).unwrap();
    fs::remove_file(f.0.join("overlay/sessions")).unwrap();
    fs::write(f.0.join("overlay/sessions"), "keep me").unwrap();
    assert!(f.prepare().unwrap_err().to_string().contains("sessions"));
    assert_eq!(f.bytes("overlay", "sessions"), b"keep me");
}

#[test]
fn missing_canonical_auth_is_not_resurrected_from_a_fork_after_logout() {
    let f = Fixture::new("missing-auth");
    let local = auth("2026-10-02T00:00:00Z", "FAKE_TOKEN", "fixture");
    f.fork("auth.json", &local, &local);
    fs::remove_file(f.0.join("global/auth.json")).unwrap();
    assert_eq!(f.prepare().unwrap_err().code, "OVERLAY_SHARED_RECOVERY");
    assert!(!f.0.join("global/auth.json").exists());
    assert_eq!(f.bytes("overlay", "auth.json"), local.as_bytes());
}

#[test]
fn ambiguous_index_and_aliased_files_are_retained() {
    let f = Fixture::new("index-tie");
    let global = rows(&[index("a", "first", "2026-10-02T00:00:00Z")]);
    let local = rows(&[index("a", "second", "2026-10-02T00:00:00Z")]);
    f.fork("session_index.jsonl", &global, &local);
    assert_eq!(f.prepare().unwrap_err().code, "OVERLAY_SHARED_RECOVERY");
    assert_eq!(f.bytes("global", "session_index.jsonl"), global.as_bytes());
    assert_eq!(f.bytes("overlay", "session_index.jsonl"), local.as_bytes());
    fs::write(f.0.join("overlay/session_index.jsonl"), &global).unwrap();
    fs::hard_link(f.0.join("overlay/session_index.jsonl"), f.0.join("alias")).unwrap();
    assert!(f.prepare().unwrap_err().to_string().contains("unaliased"));
    assert_eq!(f.bytes("global", "session_index.jsonl"), global.as_bytes());
}
