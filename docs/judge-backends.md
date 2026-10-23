# Observer judgments

Jev supplies typed Noul, Choice and Score judgments in Mine, Research and
Untrusted. Only Jev answers can drive probability or confidence gates. The
`--observer-judge` selector accepts only `jev`. Old saved
observer bindings migrate to Jev. Judgments recorded by a different backend keep
their original backend name and do not authorize current drift or completion
decisions.

## Steering and intent

Steering text is stored verbatim with its native source reference. Jev
classifies `is_steering` and `steer_kind` and selects a superseded active ID (or
`none`). Rust applies the operation. An ambiguous replacement retains both items
with explicit `uncertain_supersedes` links. The recorded goal text is never
edited; only an explicit whole-goal replacement (`goal_change`) supersedes the
goal requirement. A
confident withdrawal retains the exact withdrawal message as an active scope
constraint and the withdrawn text in history.

Canonical user instructions retain their original text. Accepted decisions are
journaled before history mutation, so an interrupted intake reuses the same
decision, and no model call happens for an already accepted cursor. Source
replay rebuilds from native user records.

## Context preparation

The source tree includes persistent native preparation of whole-record evidence.
The native observer can select whole relevant items for one decision with
recorded reasons, source pointers and digests. Preparation validates source
identities, required records and selection manifests before rendering a request.
Rust preserves the goal text and rejects the exclusion of active user or relayed
items unless a verified superseding item is cited. Source digests bind stored
evidence to the request that used it.

Automatic ledger summarization is disabled; native sessions manage their own
conversation compaction. Context rejection can stage whole-record preparation,
and a known oversized prepared body can route to direct observer review.

## Requests and the HTTP adapter

The adapter sends structured requests with `model`, `state` and `questions`. It
validates typed questions and serializes their instruction strings in the
request. Request validation checks typed content without fixed state-byte or
question-count limits; each Choice question needs 2 to 255 criteria and each
Score question 2 to 10. Responses are Noul (no confidence), Choice (choice,
probabilities, confidence) or Score (score, legend, probabilities, confidence),
with the model and input/output token usage.

Context planning uses 32,000 tokens for state plus the longest question and
64,000 tokens for the complete request as implementation thresholds. Observed
input-token usage can calibrate request estimates, but estimates are planning
data and not proof that a provider will accept a request. The runtime does not
require a local documentation mirror.

The HTTP client uses a fixed production TLS origin, disables redirects and sets a
20-second request deadline. It reads the complete response body before
validating its JSON. The host-only mode-600 key is sent as a sensitive
Authorization header; errors never render vendor bodies or credentials. The
observer crate pins `ureq` to version 3.4.2 with TLS and JSON support. kah uses
its own HTTP adapter rather than a TypeSafe SDK dependency. Jev API
authentication is separate from the native coding agent's login; keep provider
credentials in their documented host-side configuration.

Review requests may contain selected native transcript evidence, and privacy
scrubbing removes recognized credential patterns.

## Retries and failures

The scalar routing path makes at most three outer attempts for transient
failures (timeouts, transport errors, 408, 429 and 5xx), with 250 ms and 1 s
delays between attempts. The live client sends every scalar call through the
batch transport, which repeats a request answered with HTTP 429 or 529, without
a fixed limit, after the response's retry headers or its own backoff delay. Those
repeats happen inside one outer attempt. Permanent failures remain explicit
errors, while service availability state can prevent further calls. Routing
metrics count outer attempts and flag failed calls with incomplete token
accounting; latency includes backoff. The batch transport's backoff events
record each repeated HTTP request. The 20-second deadline applies to each HTTP
exchange, not to the whole call.

A service failure is recorded explicitly rather than converted into a successful
judgment. kah does not select a substitute backend when Jev fails, and a failed
or invalid judgment does not satisfy a required outcome. Adding another judgment
backend would require an explicit implementation and policy change. A different
model's probabilities are not interchangeable with the configured Jev
thresholds.

## Question design

Questions run independently against named structured state in one batch. Their
IDs are not shown to Jev, so each question's instructions must carry its full
meaning. The design follows the TypeSafe documentation on
[fan-out](https://docs.typesafe.ai/patterns/fan-out),
[intent routing](https://docs.typesafe.ai/patterns/intent-routing) and the
[building guide](https://docs.typesafe.ai/concepts/how-to-build-with-system-one).

Noul returns a yes-probability with no separate confidence. Choice and Score
retain the returned distributions and confidence; Rust computes no substitute
confidence. Score is an expectation over descriptive levels. Typed answers do not
establish truth or permission. See
[primitives](https://docs.typesafe.ai/primitives) and
[confidence](https://docs.typesafe.ai/confidence).

## Thresholds

The configured thresholds are decision policy, not an accuracy or calibration
guarantee.

- **Checkpoint and review decisions** are validated by Choice questions, for
  example whether a typed decision is supported, contradicted or unclear from
  its cited evidence; some decisions add applicability or blocker questions,
  and controller facts can skip the semantic question. A confirmed answer needs
  probability 0.90 and confidence 0.85.
- **Unattended completion** asks one Choice per outcome (satisfied, violated,
  unclear) and checks each outcome with probability 0.85 and confidence 0.80
  in both required judgments. A completion judgment must support
  every required outcome. Tests and the current user-sequence and commit checks
  remain separate mandatory checks.
- **Message typing** records steering when its presence probability is at
  least 0.80. At 0.20 or below, no steering operation is applied. Intermediate
  presence retains the exact message with `review_required`. Kind uncertainty
  is recorded separately as `kind_uncertain`. A message with clear steering
  presence can have an uncertain kind without requiring review. Replacement
  requires clear steering presence, a kind of correction, scope reduction or
  goal change, kind probability and confidence of at least 0.70, and target
  probability of at least 0.90 with confidence of at least 0.85. Only a goal
  change can replace the goal requirement. An uncertain replacement keeps both
  exact messages. Unresolved presence or replacement review flags block
  unattended completion; drift reminders continue.
  API and validation errors are recorded as gaps.

## Cost

For the `jev-1.13.0` model ID, this tree estimates input cost at $0.042 per
million tokens and assigns no output-token charge. Other model IDs get no
estimated price. Native-session token usage is separate from this estimate.
Judgment records include reported usage and elapsed time; these metrics do not
establish general model accuracy, and usage and latency records describe only
the requests measured.

## Testing

Synthetic tests check adapter request and answer formats with authored requests.
Saved request and response records can contain private evidence and should stay
local. A format test does not establish a product gating policy.
