# Native Codex overlay recovery

The native-writer references in this page describe Codex tag `rust-v0.160.0`
(`79b1b666f2e8551f8abbbca34957227f67f3f553`). The configuration reference exposes
the credential backend and SQLite directory, but no independent auth, session-index,
history or model-cache file path. File auth stays under `CODEX_HOME`; switching
keyring backend would change storage and uses a home-derived key. kah retains the
user's native backend and independent config overlay, with shared files.
Sources: [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference),
[native auth storage](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/login/src/auth/storage.rs).

| Shared entry | 0.160.0 native writer | Link behavior |
| --- | --- | --- |
| session_index.jsonl | append_thread_name appends a complete JSON record; remove_thread_name_entries writes a temporary file and renames it | Naming preserves the link; thread deletion replaces it |
| auth.json | FileAuthStorage::save opens with create/write/truncate; refresh records last_refresh then calls save | Refresh follows the link; logout removes the link at that home |
| models_cache.json | save_file uses tokio::fs::write | Follows the link |
| history.jsonl | append with native file locking; size trimming uses set_len on the same descriptor | Follows the link and keeps the inode |
| sessions / archived_sessions | Native operations address children beneath these directories | Directory links survive child writes |

Source traces: [index writer](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/rollout/src/session_index.rs),
[thread/name/set](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/app-server/src/request_processors/thread_processor.rs),
[metadata name append](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/thread-store/src/local/update_thread_metadata.rs),
[thread deletion](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/thread-store/src/local/delete_thread.rs),
[auth refresh/save](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/login/src/auth/manager.rs),
[cache writer](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/models-manager/src/cache.rs),
[history writer](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/message-history/src/lib.rs).
kah's first-turn naming and run names call the same native name API; they do not
rewrite the index themselves. A replaced overlay link is a recovery
condition; its modification time alone does not identify the operation that
replaced it. Recovery validates the current files before deciding whether they
can be merged or relinked.

Every overlay preparation distinguishes a link from a regular file before
reading a link target. Recovery runs only for the four known mutable files:

- Index: parse RFC3339 timestamps (including offsets), retain every id's newest
  entry, append missing winners to the canonical index, then relink. Equal-time
  conflicting records and incomplete/malformed JSONL are retained for resolution.
- History: append missing `(session_id, ts, text)` occurrences, preserving duplicate
  multiplicity and the canonical inode under the same file lock native history uses.
- Auth: select the whole newer same-account record by `last_refresh`, never by
  filesystem mtime and never by combining individual tokens. Replace atomically
  with mode 600 and fsync before relinking. Ties, different accounts/modes, missing
  canonical auth (possible logout) and incomplete records fail closed without
  printing values or creating a permanent credential backup.
- Models cache: discard only the replaced overlay cache and restore its link to
  the canonical disposable cache; native Codex handles expiry/refetch.

kah serializes its repairs per canonical home. Reads refuse symlinks, hard-link
aliases, nonregular files and files over 32 MiB, and check for intervening changes.
Index appends recheck newest winners because native index locking is only
in-process. Native deletion/replacement during repair is detected; the local copy
is retained for retry. History uses native-compatible locking. These controls do
not make uncooperative native filesystem operations a cross-process transaction.

The linked `rust-v0.160.0` auth writer preserves existing links when it saves a
record, so kah does not create a refresh divergence. If an auth fork already exists, recovery refuses while native Codex clients
using that fork are running. Promoting a newer fork also requires canonical-store
clients to be closed: its auth refresh lock is process-local. Keeping a newer
canonical record does not require stopping its clients. Close affected clients
and retry; do not start competing native clients during recovery. No real
token refresh is deliberately triggered by tests. Tests use synthetic credentials.

Successful recovery prints `kah repaired shared Codex file "PATH"`. Unsafe recovery
uses `OVERLAY_SHARED_RECOVERY stage=codex_overlay` and names the path and condition,
with contents withheld. Unknown replacement types and foreign links are retained.
No global config or provider binary is edited. This does not invent a new auth
store, gateway, background synchronizer or daemon policy.

Fixture cleanup clears inherited `CODEX_HOME` and uses the canonical native home
for its explicitly recorded fixture session IDs, instead of possibly rewriting an
overlay index. No user sessions are deleted.
