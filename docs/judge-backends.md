# Observer judgments

Per PLAN §8a (2026-09-30), Jev supplies typed Noul, Choice and Score judgments
in Mine, Research and Untrusted. Only Jev answers can drive probability or
confidence gates. The native text-generating Luna judge and its comparison were
removed. Historical Luna numbers are not acceptance evidence. Old saved observer
bindings migrate to Jev; old recorded judgments retain their original backend
and are refused by the drift/completion decision functions.

Source-linked intent text extraction remains a separate generative operation,
defaulting to Luna/low through Codex login. It proposes text backed by explicit
user records, never gating probabilities. Rust checks provenance and owns policy,
arithmetic, persistence and execution. Jev never generates a summary.
See [coding-agent guidance](https://docs.typesafe.ai/introduction/coding-agents)
and the [extraction cascade](https://docs.typesafe.ai/cookbooks/sde_cascade).

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
no invented price. Native text extraction through Codex login has no available
per-call cash charge. See [models and pricing](https://docs.typesafe.ai/models).
Gate 3 reports Jev labels, false reminders, missed drift, tokens and latency,
including the separate extraction overhead; it is a small journey set, not a
broad accuracy benchmark.
