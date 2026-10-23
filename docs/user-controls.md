# User controls and delegate messages

At the start of a chat message, `steer:` forces recording and `aside:` excludes
it from steering. Prefixes are case-insensitive and allow leading whitespace.
Both skip steering inference entirely and preserve the exact message text.
Type them directly in the native TUI; they are ordinary text, not shell commands.
kah records instructions separately from reviewing work. Ordinary tool and turn
events synchronize recorded instructions. Commit and checkpoint reviews evaluate
work against that intent. Ignored user sources remain in private source history
and can be reviewed or promoted. Explicit asides do not become steering
automatically.

Use `kah steer` for a live session with a reachable native control endpoint or a
recorded Herdr pane:

```sh
kah steer SESSION 'steer: Keep the API unchanged' --receipt user-change-001
kah steer SESSION 'aside: This is only a note' --receipt user-note-001
```

Leading `!` invokes the native shell in both TUIs. The old bang prefixes are no
longer deterministic overrides; old stored source decisions remain historical.

Codex control delivery uses a user-message client ID of the form
`kah-user:operator:<receipt>`; Herdr steering uses a saved relayed-user envelope.
Either way, the main agent's model, effort, project and execution policy are
preserved. `kah steer RUN TEXT` queues operator input for unattended runs, whose
source records also use the `kah-user:` namespace. kah checks native session
ownership before socket delivery and can fall back to the session's recorded
Herdr pane without resuming it. It never resumes a dormant session in a
different policy.

Review uncertainty and recent ignored input:

```
kah ledger SESSION --review
kah ledger SESSION --promote ITEM_ID
kah ledger SESSION --confirm ITEM_ID
kah ledger SESSION --dismiss ITEM_ID
```

Without SESSION, `kah ledger --review` lists known sessions. `--json` gives a
machine-readable view. Confirm accepts an active item and clears review flags;
it does not silently delete another requirement. Dismiss removes the selected
item from active intent, or excludes an ignored source from future reconsideration.
Promote makes an ignored original active, marked `user_confirmed`; the original
text remains unchanged. These local operator decisions call no judge. Inactive
and ignored originals remain retrievable with `kah ledger SESSION --json`.

Delegate messages can use a native control endpoint or a recorded Herdr pane:

```
kah send SESSION '[kah-reviewer] Review message' --sender kah-reviewer --receipt review-001
```

Every new message needs a distinct receipt. Reuse the same receipt and exact text
on retry. The Codex control path checks durable native `clientId` receipts before
retrying, handles an active-to-idle race, and keeps the text prefix for
compatibility. An uncertain transport failure stays pending; it is never blindly
resent. A native control receipt can resolve an uncertain socket delivery on
retry. If it never becomes visible, deliberate
use of a new receipt risks a duplicate and is an operator decision.

Client-ID namespaces take precedence over compatibility prefixes. Delegate and
system messages cannot become user steering, even if they contain `steer:` or `aside:`.
Raw terminal pastes lack verified sender provenance; kah's Herdr delivery uses
saved receipts and host-hook session bindings. These provenance checks assume
that processes with operator access to private host state are trusted, and they
are not authentication against another process with operator authority. Research and Untrusted executors cannot
reach the host control sockets, Herdr binary or HERDR_* environment.

Steering presence and kind have separate meanings. Jev records presence at
`>=0.8`; at `<=0.2` the source is ignored but remains promotable. Between those
bands the exact text is injected with `review_required: true`; unconfirmed
steering retains its exact text and a review flag in the current intent context. The agent continues with that unconfirmed context without waiting
for a review action. `kind_uncertain` labels an uncertain category and does not
prevent recording. `replacement_review` is separate: a proposed replacement
was not sufficiently certain, so both instructions remain active. Confirming
an item accepts its text; it does not certify its category or remove another item.

Persisted ledger events remain available as history, including unresolved review
flags, and kah does not rewrite old events or silently confirm old review flags.
User review can resolve those flags. Unresolved presence or replacement flags still conservatively
suppress automatic drift reminders and unattended completion; context injection
continues. Context preparation preserves whole source records and reports an
explicit gap when a request cannot be prepared.

The implementation defines explicit steering and replacement thresholds. kah does
not automatically retune judgment thresholds from operator review decisions.

Embedded Codex sessions do not depend on a global-daemon control socket. For an
embedded session, `kah send` and `kah steer` can use a validated Herdr binding.
Type `steer:` or `aside:` in the native TUI when no host delivery route is
available. Unattended run-ID controls use their recorded run pane, while
native-session delivery uses its available socket or Herdr binding. See
[startup modes](startup-modes.md) for the difference between saved history and
live session control.
