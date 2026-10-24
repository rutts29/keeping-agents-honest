# Observer judgments

How the observer asks its questions (is this message an instruction, does
this commit break a requirement, is this run done), which answers it trusts,
and the thresholds it applies. Read it to see why kah recorded, flagged or
blocked something.

## Jev, the judgment backend

kah asks these questions through TypeSafe's Jev API, which returns typed
answers with probabilities instead of free text. Jev is used in all three
[domains](../README.md#domains) and is the only judgment backend; the
`--observer-judge` option accepts only `jev`. Only Jev answers can drive a
probability or confidence gate. Judgments recorded by a different backend keep
their original backend name and do not authorize drift or completion
decisions.

Jev has three answer types:

- **Noul**: the probability that a statement is true. It has no separate
  confidence value.
- **Choice**: one option from a fixed list, with the probability of each option
  and a confidence value.
- **Score**: a position on a scale of described levels, with probabilities and
  a confidence value.

kah keeps the distributions and confidence values Jev returns; it computes no
substitute confidence. A typed answer is a judgment, not proof that something
is true or allowed.

## How your instructions are recorded

Your messages are stored word for word, with a reference to the session record
they came from. For each eligible message without a `steer:` or `aside:`
prefix, Jev answers whether it is steering, what kind of steering it is, and
which active instruction (if any) it replaces. The explicit prefixes bypass
that judgment.
kah's Rust code then applies that change to the intent ledger:

- If the replacement is ambiguous, kah keeps both instructions and links them
  as `uncertain_supersedes`.
- The recorded goal text is never edited. Only an explicit whole-goal
  replacement (`goal_change`) supersedes the goal.
- A confident withdrawal keeps the withdrawal message as an active scope
  constraint and keeps the withdrawn text in history.

Accepted decisions are written to a journal before the ledger changes. If
intake is interrupted, kah reuses the same decision and does not ask Jev
again. Replaying the source rebuilds the ledger from the native user records.

## Preparing context

Some decisions need evidence from a long session. A native observer session
can select whole relevant records for one decision, with recorded reasons,
source pointers and digests. kah checks those
selections before it sends a request:

- The goal text is always kept.
- Active user or relayed items cannot be left out unless a verified
  superseding item is cited.
- Digests tie the stored evidence to the request that used it.

kah does not summarize the ledger automatically; native sessions manage their
own conversation compaction. When a request's context is rejected, kah can
prepare whole records instead, and a known oversized prepared request can go
to a direct observer review.

## Requests

Each request carries a `model`, structured `state` and typed `questions`.
Choice questions need 2 to 255 options and Score questions 2 to 10 levels.
There is no fixed limit on state size or question count. Responses include the
model and the input and output token usage.

For planning, kah uses 32,000 tokens for state plus the longest question and
64,000 tokens for the whole request. These are planning estimates, not a
guarantee that the provider accepts a request.

The HTTP client uses a fixed TypeSafe production origin over TLS, does not
follow redirects and gives each request 20 seconds. Your API key (a mode-600
file on the host) is sent as a sensitive Authorization header, and error
messages never show the key or the vendor's response body. kah uses its own
HTTP adapter, built on `ureq` 3.4.2, rather than a TypeSafe SDK. The Jev key is
separate from your coding agent's login.

Review requests can include selected excerpts from the agent's session.
Recognized credential patterns are removed before sending.

## Retries and failures

- A transient failure (timeout, transport error, 408, 429 or 5xx) gets at most
  three attempts, with 250 ms and then 1 s between them.
- Within one attempt, a request answered with HTTP 429 or 529 is repeated after
  the server's retry headers or a backoff delay, with no fixed limit.
- The 20-second deadline applies to each HTTP exchange, not the whole call.
- Routing metrics count attempts and flag calls whose token accounting is
  incomplete. Latency figures include backoff time.

A failure is recorded as a failure, never turned into a successful judgment.
kah does not switch to another backend when Jev fails, and a failed or invalid
judgment never satisfies a required outcome. Another model's probabilities are
not interchangeable with the configured Jev thresholds.

## How questions are written

Questions in one batch run independently against the same named state. Jev
does not see question IDs, so each question's wording carries its full
meaning. The design follows the TypeSafe guides on
[fan-out](https://docs.typesafe.ai/patterns/fan-out),
[intent routing](https://docs.typesafe.ai/patterns/intent-routing),
[building with System One](https://docs.typesafe.ai/concepts/how-to-build-with-system-one),
[primitives](https://docs.typesafe.ai/primitives) and
[confidence](https://docs.typesafe.ai/confidence).

## Thresholds

These thresholds are kah's decision policy. They are not an accuracy or
calibration guarantee.

| Decision | What Jev answers | Required |
| --- | --- | --- |
| Checkpoint and review decisions | A Choice such as supported, contradicted or unclear from the cited evidence; some add applicability or blocker questions | Probability 0.90 and confidence 0.85 to confirm |
| Unattended completion | One Choice per required outcome: satisfied, violated or unclear | Probability 0.85 and confidence 0.80 for every outcome, in both completion judgments |
| Message is steering | Steering presence | Recorded at 0.80 or above; no steering operation at 0.20 or below |
| One instruction replaces another | Kind of steering, then which item it replaces | Steering presence at least 0.80; kind is correction, scope reduction or goal change, with probability and confidence of at least 0.70; target probability at least 0.90 and confidence at least 0.85 |

Details behind the table:

- Controller facts can settle a review decision without a semantic question.
- Tests and the current user-sequence and commit checks are separate,
  mandatory completion checks.
- Between 0.20 and 0.80 steering presence, the message is kept word for word
  and marked `review_required`.
- An uncertain kind is recorded as `kind_uncertain`. A message that is clearly
  steering can have an uncertain kind without needing review.
- Only a goal change can replace the goal. An uncertain replacement keeps both
  messages.
- Unresolved presence or replacement review flags block unattended completion;
  drift reminders continue. API and validation errors are recorded as gaps.

## Cost

For the `jev-1.13.0` model ID, kah estimates input cost at $0.042 per million
tokens, with no charge for output tokens. Other model IDs get no estimate.
Usage by native Claude Code or Codex sessions is separate. Judgment records
include the reported usage and elapsed time of each request; they describe
only the requests measured.

## Saved requests

Saved request and response records can contain private evidence. Keep them
local.
