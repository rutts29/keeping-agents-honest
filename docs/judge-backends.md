# Observer judgments

Per PLAN §8a (2026-09-30), Jev supplies typed Noul, Choice and Score judgments
in Mine, Research and Untrusted. Only Jev answers can drive probability or
confidence gates. The native text-generating Luna judge and its comparison were
removed. Historical Luna numbers are not acceptance evidence. Old saved observer
bindings migrate to Jev; old recorded judgments retain their original backend
and are refused by the drift/completion decision functions.

Per §8b, steering is now stored verbatim with its native record reference.
Jev classifies `is_steering` and `steer_kind` and selects a superseded active ID
(or `none`). Rust applies the operation. Ambiguous replacement retains both
items with explicit `uncertain_supersedes` links. The original goal never changes.
A confident withdrawal retains the exact withdrawal message as an active scope
constraint and the withdrawn text in history.

Luna/low synthesis is now **compaction only**, above the 32 KiB rendered budget.
Its separate prompt summary links to original item IDs and a content digest;
canonical items and the goal remain unchanged. `kah ledger SESSION` retrieves
history, including inactive originals. Jev completion reads canonical text, never
the summary, and unresolved replacement flags block completion.

New verbatim runs have a separate stable identity. Existing synthesized ledgers
remain on disk; source replay rebuilds from native user records rather than
claiming old generated text was verbatim. Accepted decisions are journaled before
history mutation so an interrupted intake reuses the same decision. No model
call happens for an already accepted cursor.

Local reference: `~/.local/share/typesafe-docs`, pinned 2026-09-30. Checked against
`sdk/js-v0.6.0/types.ts`: kah sends `{model,state,questions}`; its nonempty string
instructions/criteria are a supported subset of SDK `EntryType`. Responses match
Noul (no confidence), Choice (choice/probabilities/confidence), Score
(score/legend/probabilities/confidence), model and input/output token usage.
kah's stricter request limits are application bounds. Oversize inputs record a
gap; candidate IDs are never silently dropped. No mirror auto-update is used.

The HTTP adapter follows the [API reference](https://docs.typesafe.ai/api),
[Python](https://docs.typesafe.ai/sdk/python) and
[JavaScript](https://docs.typesafe.ai/sdk/javascript) examples. It uses fixed TLS
origin, no redirects, a 20-second request deadline and a bounded response. The
host-only mode-600 key is a sensitive Authorization header; errors never render
vendor bodies or credentials. ureq 3.4.2 is maintained (repository dates to 2018)
and covered by cargo audit. No TypeSafe SDK or other TypeSafe feature is used.

429, 529, other server errors and transport/timeouts get at most two retries,
after 250 ms and 1 s. Permanent errors are not retried. Exhaustion becomes an
explicit observer gap; no different model supplies substitute numbers. Successful
retry metrics count all attempts and flag failed calls with incomplete token
accounting. Latency includes backoff. Bounded context excludes transcripts and
credentials in every domain.

Questions run independently against named structured state in one batch. Their
IDs are not shown to Jev, so the instructions carry their full meaning. These
choices follow [fan-out](https://docs.typesafe.ai/patterns/fan-out),
[intent routing](https://docs.typesafe.ai/patterns/intent-routing), and the
[building guide](https://docs.typesafe.ai/concepts/how-to-build-with-system-one),
plus typesafe-ai/skills@65a39f3.

Noul returns a yes-probability, with no separate confidence. Choice/Score retain
returned distributions and confidence; Rust computes no substitute confidence.
Score is an expectation over descriptive levels. Typed answers do not establish
truth or permission. Thresholds remain provisional, evaluated by the live
journeys rather than treated as calibration guarantees. See
[primitives](https://docs.typesafe.ai/primitives),
[confidence](https://docs.typesafe.ai/confidence), and
[current limitations](https://docs.typesafe.ai/model-jaggedness/jev-1.13).

For returned snapshot jev-1.13.0, the estimated cost uses the documented $0.042
per million input tokens, output free (checked 2026-09-29). Unknown snapshots get
no invented price. Native compaction through Codex login has no available
per-call cash charge. See [models and pricing](https://docs.typesafe.ai/models).
Gate 3 reports Jev labels, false reminders, missed drift, tokens and latency,
including any size-triggered compaction overhead; it is a small journey set, not a
broad accuracy benchmark.

Phase 4 question set 4 asks one Choice per requirement (satisfied, violated,
unclear). Code combines all required results at the unchanged 0.90 probability
and 0.85 confidence thresholds; two independent calls must agree. A redundant
whole-task classification was removed after it vetoed a live candidate whose
individual requirements all passed. Tests and current user-sequence/commit checks
remain independent mandatory gates. No generation model supplies the numbers.

Gate 3b question set 5 separates message typing (before deterministic intake)
from action assessment (after intake). The dependency requires two calls when
both are new; independent questions in each call still fan out. Confident steering uses
Noul >=0.80. At <=0.20, no steering operation is applied. Intermediate presence
or low kind confidence retains the exact message with `review_required` and
uncertainty links; it cannot replace anything or authorize automated completion
or drift reminders. Later source records still advance. Replacement needs kind probability/confidence
>=0.70 and target
probability >=0.90 and confidence >=0.85. API/validation errors record gaps; uncertain replacement preserves both exact messages. These are evaluated policy thresholds,
not a general accuracy guarantee.
