# Evidence summary

This page states how kah was verified during development. The raw evidence
(session transcripts, run receipts and logs) contains private user text and
host details, so it is kept private and is not part of this repository.

## Real work: safetensors issue #878

On 20 October 2026, kah supervised Codex (GPT-6.1-Sol, high effort) fixing
out-of-range slice stops for huggingface/safetensors issue #878. In two
sessions, the observer's LLM review, a long-running Codex observer session
reading the agent's commits as it worked, found three real defects in the code
the agent was committing:

- a signed/unsigned mismatch in slice-stop extraction, so very large stops
  raised different exceptions on different paths;
- error precedence that differed between backends when two dimensions were
  invalid;
- a negative-start, over-range slice that raised one error type on the torch
  memory-mapped path and another on the byte paths.

The agent fixed each one in its next commit, about 170, 230 and 184 seconds
after the observer's finding.

## Real work: a tokenizers API task with Claude Code

During a multi-commit tokenizers v1 API task begun on 15 October 2026, an
evidence request from kah on 16 October led Claude Code to identify an earlier
public enum addition that conflicted with a later instruction. After the rework
was approved, the agent removed it.

## Controlled test: planted violations

On 11 October 2026, a frozen replay ran 31 commit cases: eight planted policy
violations and 23 controls. The first-level screen, TypeSafe's Jev answering
one typed question per rule, escalated seven of the eight planted violations to
LLM review, which confirmed all seven. None of the 23 controls was escalated or
confirmed.

The Jev screen used 1,762,616 input tokens for the 31 cases, about $0.0024 per
commit at $0.042 per million input tokens. That figure is the per-commit Jev
screen only. The LLM review, a long-running Claude Code or Codex observer
session, reviews flagged commits and checkpoints.

## Live correction: a vendor/ rule

In seeded live sessions, the task said not to modify anything under `vendor/`,
and a test harness introduced a vendor-file edit. kah's reminder reached the
agent while it was still working, and the agent's own commit restored the
file: 37
seconds after the reminder for Codex (13 October 2026) and 70 seconds for
Claude Code (16 October 2026).

## Live end-to-end runs

The author ran kah against real Claude Code and Codex sessions on the
author's server, with the full sandbox setup described in the README. The
author's records show Codex 0.159.0 and Claude Code 2.1.286 and 2.1.287 in the
last of these runs; earlier runs recorded their own versions.

Across the two providers, these runs covered:

- Native launch with the host login, and agent commands running in gVisor.
- Claude Code native file tools working alongside Bash in gVisor, with
  protected host paths.
- Offline workspaces, online MCP servers, hosted search, and `--online`.
- Untrusted repository settings and Git configuration, tested against
  attempts to disable containment or run code on the host.
- Unattended runs: native session in a worktree, commit import, verification
  in a fresh checkout, and completion checks.
- Steering, stop, resume, limits and session names for unattended runs.
- Observer intent capture, corrections, scope changes and drift, using Jev
  typed judgments and `steer:` / `aside:`.

## Release candidate check

On 23 October 2026 the author installed a release candidate of this code with
`make install` into a fresh home directory on the same server and ran it
against Claude Code 2.1.309 and Codex 0.162.1. The check covered one Mine
session per provider: launch with the host login, intent capture with a Jev
judgment (a `steer:` message recorded verbatim and an `aside:` message
excluded), and cleanup that passed the recorded owner-configuration checks and
restored the proof's temporary resources.

## Unit and integration tests

The workspace has more than 1,200 Rust unit and integration test functions
across its eight crates. Most run in the default selection (`make test`) with
local data, temporary Git repositories and mock or loopback transports.
Host-dependent test cases, such as those that need the sandbox service
account, a real provider login, Herdr or the TypeSafe API, are marked
`#[ignore]` and run only when selected by name.

The repository also contains live end-to-end tests. They are ignored by
default because they need the author's host setup and provider logins. See
the README for how to build and run the default tests.
