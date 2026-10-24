# Relays and dispatch

How a user instruction given in one session reaches another: steering relayed
by a host orchestrator, quote-verified relays with `kah relay`, and the
requirements a dispatched run inherits from its parent session.

Run these commands on the host. Both sessions must run under kah with its
current host hooks; a session started with an older hook configuration needs a
normal kah resume first. Use the native session ID, not a Herdr display name.

```sh
kah steer RECEIVER_SESSION 'The user decided to change the output to RED.'
kah relay sources SENDER_SESSION
kah relay SENDER_SESSION RECEIVER_SESSION MESSAGE_ID 'Sender paraphrase'
kah run --parent PARENT_SESSION --test 'your test command' 'Worker assignment'
```

## Relayed steering

When a host orchestrator passes on an explicit user decision, `kah steer`
delivers it; an embedded session without a control socket receives it through
its recorded Herdr pane.

- kah records the exact text as user intent with `origin=relayed`. The
  `relayer` is the caller's Herdr agent name, or `operator` for an unnamed host
  caller. `kah ledger` shows the relayer, including in its JSON provenance.
- Jev still classifies steering, kind and supersession. Relaying does not force
  a kind or lower any threshold.
- No sender registration or quote matching is needed. `--receipt ID` makes a
  retry idempotent.

This trusts host-side submission; it is not proof of what the user said.
Research and Untrusted tools have no Herdr socket, and kah's state is outside
their writable boundary (see [domains](../README.md#domains)). Use it only when
you are authorized to submit the user's instruction. The relayer is a courier,
not a lower authority. Plain `[kah-reviewer]` messages stay delegate context
and cannot enter the user ledger this way.

## Quote-verified relays

Use `kah relay` when the receiver should get the user's original words,
verified against the sender's transcript.

- `kah relay sources SESSION` lists the user messages that can be relayed.
  The message ID is the source record ID (for Claude, a native message UUID,
  possibly with a content-block suffix).
- kah copies the complete selected user message verbatim from the sender's
  native transcript, with provider, session and message provenance, and
  validates its source identity and content digest. A model never extracts or
  summarizes it. Delegate messages, tool results, summaries and subagent
  messages cannot supply a quote.
- Jev checks whether your paraphrase is supported by the quote. Below 0.90
  support probability, or if the judge fails, the paraphrase is labeled
  `UNCONFIRMED`. Even a supported paraphrase stays delegate context. The exact
  quote goes through normal `steer:` / `aside:` handling and intent intake
  either way.

### Delivery and receipts

- kah saves a private receipt before delivery, then uses the session's Codex
  control endpoint when available, otherwise its recorded Herdr pane. It never
  probes the global Codex daemon.
- The receiving hook compares the whole delivered message with the saved
  receipt before importing the quote. A forged prefix, wrong recipient or
  changed text gains no user authority. Claude's multiline paste framing is
  decoded only as one complete block, and its content must still match.
- The CLI prints the receipt ID and the transport result. Submission is not
  acknowledgment: `received-RECEIPT.json` in the private observer session
  directory records the match. Unknown transport outcomes are not retried
  automatically.

## Dispatch: inherited requirements

`kah run --parent SESSION` starts a worker run that inherits requirements from
a parent session. Without `--parent`, kah uses the native thread inside Codex,
or kah's hook binding inside a Herdr agent pane. From an ordinary shell with no
parent, the run is independent.

What the worker inherits from the parent's active items:

- Always: constraints, invariants, prohibitions, validation rules, stop
  conditions, corrections and scope reductions.
- Also kept: items with an uncertain category, and inherited scope whose
  original category is unavailable.
- Filtered by Jev for relevance: only clear goal or outcome items and scope
  expansions. Uncertain relevance is kept.

There is no selection or count limit. An unreadable or invalid inherited source
fails the run before the worker starts; requirements are never dropped
silently. Items stay whole with their original source references, so split
unrelated requirements in the parent ledger when their scopes need to differ.

Before the worker's first tool call, kah copies the selected items into the
worker's ledger with their source pointers, categories and uncertainty, and
the startup hook gives the worker the inherited quotes before it plans. The
worker's assignment is its goal. The parent's completion evidence and
replacement links do not carry over, and uncertain parent relationships stay
flagged for review. Imports survive resume and are deduplicated by source; a
resumed worker uses its current ledger rather than the parent context again.

## Trust boundary

kah's private host state is the trust boundary for receipts, and restricted
tools cannot write it. This does not authenticate against another process that
is already trusted to write your host state.
