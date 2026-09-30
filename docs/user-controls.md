# User controls and delegate messages

At the start of a chat message, `steer:` forces recording and `aside:` excludes
it from steering. Prefixes are case-insensitive and allow leading whitespace.
Both skip steering inference entirely and preserve the exact message text.
Type them directly in the native TUI; they are ordinary text, not shell commands.
Normal observation of subsequent actions still uses Jev. Ignored messages stay
in private source history and remain promotable. Recent model-ignored messages
are included in bounded drift context; explicit asides and dismissed/promoted
sources are excluded from that recheck.

For a live, initialized session with a reachable native control socket, the
out-of-band path remains (embedded kah TUIs currently have no such socket):

```sh
kah steer SESSION 'steer: Keep the API unchanged' --receipt user-change-001
kah steer SESSION 'aside: This is only a note' --receipt user-note-001
```

Leading `!` invokes the native shell in both TUIs. The old bang prefixes are no
longer deterministic overrides; old stored source decisions remain historical.

This uses the supported native message API with `kah-user:operator:<receipt>`;
it preserves the main agent's model, effort, project and execution policy.
`kah steer RUN TEXT` still queues operator input for unattended runs, whose source
records also use the `kah-user:` namespace. Native senders use the session's saved
overlay socket, or the global daemon socket for sessions without a kah overlay;
they never resume a dormant session in a different policy.

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

Delegate delivery to a reachable native control socket:

```
kah send SESSION '[kah-reviewer] Review message' --sender kah-reviewer --receipt review-001
```

Every new message needs a distinct receipt. Reuse the same receipt and exact text
on retry. The sender checks durable native `clientId` receipts before retrying,
handles an active-to-idle race, and keeps the text prefix for compatibility.
An uncertain transport failure stays pending; it is never blindly resent. A
visible receipt resolves it on retry. If it never becomes visible, deliberate
use of a new receipt risks a duplicate and is an operator decision.

Client-ID namespaces take precedence over compatibility prefixes. Delegate and
system messages cannot become user steering, even if they contain `steer:` or `aside:`.
Herdr's unmarked terminal paste has no recoverable sender metadata; native source
IDs avoid that ambiguity for the new channel. These are provenance conventions
within the accepted single-user host trust model, not authentication against
another process with operator authority. Research and Untrusted executors cannot
reach the host control sockets, Herdr binary or HERDR_* environment.

Steering presence and kind have separate meanings. Jev records presence at
`>=0.8`; at `<=0.2` the source is ignored but remains promotable. Between those
bands the exact text is injected with `review_required: true`, including after
compaction. The agent continues with that unconfirmed context without waiting
for a review action. `kind_uncertain` labels an uncertain category and does not
prevent recording. `replacement_review` is separate: a proposed replacement
was not sufficiently certain, so both instructions remain active. Confirming
an item accepts its text; it does not certify its category or remove another item.

Existing persisted decisions remain historical; this change does not rewrite
old ledger events or silently confirm old review flags. User review can resolve
those flags. Unresolved presence or replacement flags still conservatively
suppress automatic drift reminders and unattended completion; context injection
continues. If exact unconfirmed text itself exceeds the compaction budget, kah
reports an explicit compaction gap instead of silently dropping it.

Thresholds are explicit project policy. Future retuning uses actual
operator review decisions after about a week of use, with a development/validation
split and misses-first comparison. It is never automatic.


Gate 3f verified that embedded kah TUIs have no control socket. Their history is
shared, but kah send and native-session kah steer fail closed rather than delivering
through the global daemon. Use typed steer:/aside: in those TUIs. Native delivery
to the existing managed-daemon sessions and run-ID steering are separate paths.
See startup-modes.md for the pending shared Mine proof and app visibility limits.
