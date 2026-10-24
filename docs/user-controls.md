# User controls and delegate messages

How user messages become recorded instructions, how to send messages to a
live session, and how to review the ledger.

## `steer:` and `aside:`

Type these at the start of an ordinary chat message in the agent's own
interface:

- `steer:` always records the message as an instruction.
- `aside:` marks the message as commentary, so it never becomes an
  instruction automatically.

Prefixes are case-insensitive and may follow leading whitespace. Both skip
kah's steering judgment and keep the exact message text. A leading `!` is the
native shell syntax in both agents, not a kah command.

Messages without a prefix are classified by a Jev judgment (see
[steering bands](#steering-bands)). kah records instructions separately from
reviewing work: commit and checkpoint reviews check the agent's work against
the recorded instructions.

## Sending to a live session

`kah steer` sends your message to a live session. It needs either a reachable
native control endpoint or the Herdr pane kah recorded for the session:

```sh
kah steer SESSION 'steer: Keep the API unchanged' --receipt user-change-001
kah steer SESSION 'aside: This is only a note' --receipt user-note-001
```

- The agent keeps its model, effort, project and execution policy.
- kah checks that the session is the one it owns before using its control
  socket. Otherwise it can use the session's recorded Herdr pane without
  resuming the session. It never resumes a dormant session in a different
  policy.
- `kah steer RUN TEXT` queues your message for an unattended run. The run
  controller types it into the run's pane and confirms it by matching the exact
  text in the native history.
- With no delivery route, type `steer:` or `aside:` directly in the agent's
  chat. See [startup modes](startup-modes.md) for the difference between saved
  history and live control.

## Reviewing the ledger

The ledger is kah's per-session record of user instructions, each with its
exact text and source. Review uncertain items and recently ignored input:

```
kah ledger SESSION --review
kah ledger SESSION --promote ITEM_ID
kah ledger SESSION --confirm ITEM_ID
kah ledger SESSION --dismiss ITEM_ID
```

- `--review` lists every item awaiting review and up to the 12 most recent
  ignored, aside or gap sources. Without SESSION, `kah ledger --review` lists
  known sessions. `--json` gives a machine-readable view.
- **Confirm** accepts an active item and clears its review flags. It does not
  delete another requirement, and it does not certify the item's category.
- **Dismiss** removes the item from active intent, or stops an ignored source
  from being reconsidered.
- **Promote** makes an ignored message active, marked `user_confirmed`. The
  original text stays unchanged.

These are local decisions; they call no judge. Inactive and ignored originals
remain available with `kah ledger SESSION --json`. kah never rewrites old
ledger events or confirms old review flags on its own.

## Steering bands

For a message without a prefix, Jev gives the probability that it is an
instruction:

| Probability | Result |
| --- | --- |
| 0.8 or more | Recorded as an instruction. |
| 0.2 or less | Ignored, but kept and promotable. |
| In between | Recorded with its exact text and `review_required: true`. The agent continues with it without waiting for your review. |

Two further flags are separate from presence:

- `kind_uncertain`: the instruction's category is uncertain. It is still
  recorded.
- `replacement_review`: a proposed replacement of an earlier instruction was
  not certain enough, so both instructions stay active.

Unresolved presence or replacement flags block unattended completion
(`RUN_INTENT_UNCERTAIN`) until you review them. Drift reminders continue
meanwhile. kah does not retune its thresholds from your review decisions.

## Delegate messages

A delegate message comes from another agent or tool acting for you, not from
you. Send one with `kah send`, through a native control endpoint or a recorded
Herdr pane:

```
kah send SESSION '[kah-reviewer] Review message' --sender kah-reviewer --receipt review-001
```

- The sender defaults to `kah-builder`. A sender is `kah-` followed by
  lowercase letters, digits or `-`, at most 32 characters in all. `kah-user`
  is reserved and refused.
- The `[sender]` prefix marks the message as delegate context for both agents.
- Delegate and system messages never become user instructions, even if they
  contain `steer:` or `aside:`.

## Receipts and retries

A receipt is a message ID that makes retries safe.

- A receipt is at most 128 letters, digits, `.`, `_` or `-`.
- Without `--receipt`, kah generates one and prints it. A retry without that
  same receipt is a new message.
- Give every new message a new receipt. On retry, reuse the same receipt and
  the exact same text.
- If delivery fails in a way that leaves the outcome uncertain, the message
  stays pending and is never blindly resent. A retry with the same receipt can
  resolve it. If it never appears, sending it again under a new receipt may
  duplicate it; that is your decision.

## Trust boundary

Raw terminal pastes carry no verified sender, so kah's Herdr delivery uses
saved receipts and session bindings recorded by kah's host hooks. These checks
assume that processes with access to your private host state are trusted; they
do not authenticate against another process with that access. Agent commands
in Research and Untrusted projects (see [domains](../README.md#domains)) cannot
reach the host control sockets, the Herdr binary or the `HERDR_*` environment.
