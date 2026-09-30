# Typed observer judgments

The `Judge` interface asks bounded Noul, Choice and Score questions. Rust owns
provenance, arithmetic, policy, reminders and execution. Jev supplies typed
judgments; Luna through the existing Codex login is the comparison/fallback
backend. Source-linked text extraction remains a separate generative operation:
Jev does not generate summaries. See TypeSafe's [coding-agent guide](https://docs.typesafe.ai/introduction/coding-agents)
and [extraction cascade](https://docs.typesafe.ai/cookbooks/sde_cascade).

The HTTP adapter follows the [API reference](https://docs.typesafe.ai/api), checked
against both [Python](https://docs.typesafe.ai/sdk/python) and
[JavaScript](https://docs.typesafe.ai/sdk/javascript) examples. It uses a fixed TLS
origin, no redirects, a 20-second deadline and a bounded response. The host-only
mode-600 key is held in a sensitive Authorization header. Neither request/error
logs nor evidence contain it; error bodies are never rendered. The only new HTTP
crate is ureq 3.4.2: its maintained repository dates to June 2018; cargo audit is
part of every commit check. No TypeSafe SDK or other TypeSafe feature is used.

Questions run independently against a shared state. Put complete meaning in the
instructions, since question IDs are not shown to the model. Keep source, prior
intent, current intent and native actions in named fields. Batch independent
checks in one call, and consume only relevant results. These choices follow
[fan-out](https://docs.typesafe.ai/patterns/fan-out),
[intent routing](https://docs.typesafe.ai/patterns/intent-routing), the
[building guide](https://docs.typesafe.ai/concepts/how-to-build-with-system-one),
and the installed typesafe-ai skill at typesafe-ai/skills@65a39f3.

Noul exposes its yes-probability, with no invented confidence field. Choice and
Score retain the returned distribution and confidence. Score is an expectation
on descriptive levels, not a measurement of a physical quantity. Luna generates
probability/confidence estimates; Rust computes its selected option and weighted
score. Those estimates must not be described as empirically calibrated.
[Primitive semantics](https://docs.typesafe.ai/primitives) and
[confidence](https://docs.typesafe.ai/confidence) define these distinctions.

The [current limitations](https://docs.typesafe.ai/model-jaggedness/jev-1.13)
matter: Jev can be literal, loses accuracy with irrelevant context, and can be
influenced by adversarial state. Typed output does not make it an authority or a
sandbox boundary. Exact source matching is deterministic; semantic support is a
separate question, as in the [citation cookbook](https://docs.typesafe.ai/cookbooks/citation_check).
Keep thresholds with the questions and evaluate false reminders and missed drift;
do not copy cookbook example thresholds as established accuracy guarantees.

Live contract probe: first call had valid types but correction probability 0.63
without prior-intent context, below the provisional 0.8 expectation. The next
call included prior intent and returned 0.98, with correct drift/score answers.
Evidence: gates/evidence/phase-3/jev-contract-first.json and jev-contract.json.
This is not the Gate 3 Jev/Luna journey comparison, which remains outstanding.

For the returned model jev-1.13.0, estimated cost uses the documented $0.042 per
million input tokens, output free, checked 2026-09-29. An unknown returned model
gets no invented price. The Codex-login baseline has no per-call cash charge
available to measure, so its dollar estimate remains null. See [models and pricing](https://docs.typesafe.ai/models).

Jev records the returned model snapshot. The native Codex baseline records its
configured model slug; the CLI does not provide a separate deployment snapshot.
