# Dispatch and verified user relays

Run these commands on the host. Both sessions must run under kah with the current native host hooks enabled.
Use the native session ID, not a Herdr display name. A session started with an
older hook configuration needs a normal kah resume to pick up new hook events.

```sh
kah relay sources SENDER_SESSION
kah relay SENDER_SESSION RECEIVER_SESSION MESSAGE_ID 'Sender paraphrase'
kah run --parent PARENT_SESSION --test 'your test command' 'Worker assignment'
```

Relay reads the selected user text directly from the sender's native transcript.
The message ID is the adapter's source record ID (for Claude, a native message
UUID may have a content-block suffix). Delegate messages, tool results, summaries
and subagent messages cannot supply a user quote. The complete selected text is
copied verbatim with provider/session/message provenance; it is never extracted
or summarized by a model. Quotes are limited to 16 KiB.

Jev checks whether the separate paraphrase is supported by the quote. Below 0.90
support probability, or on judge failure, the paraphrase is labeled UNCONFIRMED.
Even a supported paraphrase remains delegate context. The exact quote remains
eligible for normal `steer:` / `aside:` and intent intake regardless of that check.

kah saves a private receipt before delivery. It uses an existing session-specific
Codex native control endpoint when available, otherwise the recorded Herdr socket
and pane. It never probes the global daemon. kah's primary host hook binds the
pane to its native session; Herdr process-discovery IDs are advisory. The receiving
hook compares the entire delivered envelope to the saved receipt before importing
the quote. A forged prefix, wrong recipient or changed text gains no user authority.
Claude's native multiline paste framing is decoded only as one complete block;
its content must still match the receipt exactly.

The CLI prints the receipt ID and transport result. Submission is not a receiver
acknowledgment: `received-RECEIPT.json` in the private observer session directory
records receipt matching. Unknown transport outcomes are not automatically retried.
The original receipt and delivery diagnostic remain private for inspection.

For dispatch, `--parent` selects the parent explicitly. Inside Codex, kah uses the
native thread environment; inside a Herdr agent pane it uses kah's native host-hook
binding. An ordinary shell without a parent starts an independent run. Constraints,
invariants, prohibitions, validation rules, stop conditions, corrections and scope
reductions are always inherited. Uncertain categories and inherited scope whose
original expansion/reduction category is unavailable are retained conservatively.
Only clear goal/outcome or scope-expansion items are relevance-filtered by Jev;
uncertain relevance is retained. Selection examines at most 24 active items
and imports at most 22, reserving the worker's own goal/task outcomes;
an overfull or unreadable source fails before starting a worker, never silently
drops requirements. Items stay whole, so split unrelated requirements in the
parent ledger when their scopes need to differ.

Before the first native tool, kah copies selected items into the worker ledger
with original source pointers, categories and uncertainty. It preserves the worker
assignment as the goal; parent completion evidence and replacement relationships
do not become worker completion evidence. Uncertain parent relationships remain
flagged for review. The completion controller also imports the items before
evaluating a tool-free turn. Imports survive native resume and deduplicate by
original source identity. Receipt intake uses a separate short I/O lock, so an
in-flight background judge cannot stall receipt recording.
The native startup hook supplies the selected quotes before the worker plans its
first action. This initial context is limited to 48 KiB and is checked before the
run starts. Native resume uses the current ledger instead of reinjecting an older
parent context.

Private kah host state is the trust boundary for receipts. Restricted tools cannot
write it. This does not authenticate against another process already trusted to
write the user's host state, and it does not retroactively promote old free-form
reviewer messages that lack verified source pointers.
