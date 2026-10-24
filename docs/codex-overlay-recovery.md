# Native Codex overlay recovery

A kah overlay is an independent writable Codex configuration per project that
links your login, history and session index to your canonical Codex home.
This page covers what happens when native Codex replaces one of those links
with a regular file, and what `kah repaired shared Codex file` and
`OVERLAY_SHARED_RECOVERY` mean.

## What is shared

Codex's configuration lets you move the credential backend and the SQLite
directory, but has no separate path setting for auth, the session index,
history or the model cache. File auth stays under `CODEX_HOME`, and switching
to the keyring backend would change storage and use a home-derived key. So kah
keeps your native backend, gives each overlay its own config, and links the
shared files.

How native Codex writes each shared entry decides whether the link survives:

| Shared entry | How native Codex writes it | Effect on the link |
| --- | --- | --- |
| session_index.jsonl | Naming appends a complete JSON record; thread deletion writes a temporary file and renames it | Naming keeps the link; deletion replaces it |
| auth.json | Save opens with create/write/truncate; a refresh records `last_refresh` and then saves | Refresh follows the link; logout removes the link in that home |
| models_cache.json | Written with `tokio::fs::write` | Follows the link |
| history.jsonl | Appended under native file locking; trimming uses `set_len` on the same descriptor | Follows the link and keeps the inode |
| sessions / archived_sessions | Native operations write children beneath these directories | Directory links survive child writes |

kah's first-turn names and run names use the same native naming API; kah does
not rewrite the index itself.

## When a link is replaced

A replaced link needs recovery. Its modification time alone does not show
which operation replaced it, so kah validates the current files before it
merges or relinks them. Every overlay preparation checks whether an entry is a
link or a regular file before reading a link target. Recovery covers only the
four known mutable files:

- **Session index.** kah parses RFC3339 timestamps (including offsets), keeps
  the newest entry for every id, appends missing winners to the canonical
  index, then relinks. Equal-time conflicting records and incomplete or
  malformed JSONL are kept for you to resolve.
- **History.** kah appends missing `(session_id, ts, text)` entries, keeping
  duplicates and the canonical inode, under the same file lock native history
  uses.
- **Auth.** kah selects the whole newer record for the same account by
  `last_refresh`, never by file modification time and never by mixing
  individual tokens. It replaces the file atomically with mode 600 and fsyncs
  before relinking. Ties, different accounts or modes, missing canonical auth
  (possibly a logout) and incomplete records stop recovery without printing
  values or keeping a permanent credential backup.
- **Models cache.** kah discards only the replaced overlay copy and restores
  the link to the canonical cache; native Codex refetches it when needed.

## Safety limits

- kah repairs one canonical home at a time.
- Reads refuse symlinks, hard-link aliases, non-regular files and files over
  32 MiB, and check for changes made in between.
- Index appends recheck the newest winners, because native index locking works
  only within one process.
- If native Codex deletes or replaces a file during repair, kah notices and
  keeps the local copy so you can retry.
- These checks do not turn uncooperative native file operations into a
  cross-process transaction.

The native auth writer keeps existing links when it saves, so kah does not
create a refresh split. If an auth split already exists:

- Recovery refuses while native Codex clients that use the split copy are
  running.
- Promoting a newer split copy also requires clients of the canonical store to
  be closed, because their refresh lock is process-local.
- Keeping a newer canonical record does not require stopping its clients.

Close the affected clients and retry. Do not start other native clients during
recovery.

## Messages you may see

- `kah repaired shared Codex file "PATH"`: recovery succeeded.
- `OVERLAY_SHARED_RECOVERY stage=codex_overlay`: recovery was unsafe. The
  message names the path and the condition, without file contents.

Unknown replacement types and foreign links are left in place. kah never edits
your global Codex config or the Codex binary.

## References

The writer behavior above is from Codex `rust-v0.160.0` source:
[configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference),
[native auth storage](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/login/src/auth/storage.rs),
[index writer](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/rollout/src/session_index.rs),
[thread/name/set](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/app-server/src/request_processors/thread_processor.rs),
[metadata name append](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/thread-store/src/local/update_thread_metadata.rs),
[thread deletion](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/thread-store/src/local/delete_thread.rs),
[auth refresh/save](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/login/src/auth/manager.rs),
[cache writer](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/models-manager/src/cache.rs),
[history writer](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/message-history/src/lib.rs).
