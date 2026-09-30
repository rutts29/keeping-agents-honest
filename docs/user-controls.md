# User controls and delegate messages

`!steer ` forces a literal user message into the ledger with uncertainty cleared;
`!aside ` excludes it from steering. Both skip steering inference entirely.
Normal observation of subsequent agent actions still uses Jev. Ignored messages
remain verbatim in private source history, including when no goal exists yet.
Recent model-ignored messages are included in bounded drift context. Explicit
asides, dismissed sources and already-promoted sources are excluded from that
recheck.

Codex 0.159's **typed TUI bang prefix runs a shell command**. Do not type these
prefixes as bare TUI commands. Submit a literal message to a live, already
initialized conversation from another shell:

```
kah steer SESSION '!steer Keep the API unchanged' --receipt user-change-001
kah steer SESSION '!aside This is only a note' --receipt user-note-001
```

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

Delegate delivery:

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
system messages cannot become user steering, even if they contain `!steer`.
Herdr's unmarked terminal paste has no recoverable sender metadata; native source
IDs avoid that ambiguity for the new channel. These are provenance conventions
within the accepted single-user host trust model, not authentication against
another process with operator authority. Research and Untrusted executors cannot
reach the host control sockets, Herdr binary or HERDR_* environment.
