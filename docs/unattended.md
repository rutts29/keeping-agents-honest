# Unattended work

Start inside Herdr, from a project under `~/repos/{mine,research,untrusted}`:

```sh
kah run --agent claude --test 'make check' 'Implement the requested change'
kah run --model MODEL --effort EFFORT --time-limit 600 --test 'make check' 'Implement the change'
kah steer run-ID 'steer: Use the revised behavior instead'
kah status run-ID
kah logs run-ID
kah stop run-ID
kah resume run-ID
```

Gate 5r-i proves full native launch and verified completion for both providers.
Control, resume and limit journeys are still being completed; see [status](STATUS.md).
The default agent is Codex. Model and effort are optional native choices. The
observer's compactor and Jev alias use the separate `--observer-model`,
`--observer-effort`, and `--jev-model` options.

A run starts at committed HEAD in a private worktree and its own Herdr pane.
Uncommitted source edits are not imported. The task is the first native prompt;
there is no exec/print worker or required JSON reply. Native consent and prompts
remain native. Fresh worktree paths currently require a person to accept startup
consent for both providers before work can proceed. kah shows a Herdr notification
when the agent is blocked, and a startup notice after 15 seconds without the first
task submission. `kah status` records the wait and the native pane ID. Attach to
that pane to answer; kah never sends consent keys. The wait clears after submission.
Logs show that pane's recent output. Run time limits continue during this wait.
Notifications honor Herdr's configured `[ui.toast]` delivery, including `off`;
kah also prints the consent notice at launch and retains the wait in status.

Claude 2.1.286 has no documented invocation-local interactive trust flag; print
mode skips trust but would change the required native session flow. Its trust
acceptance is project state in ~/.claude.json, not a --settings property. kah does
not edit that file. Codex 0.159.0 also asks for consent when an embedded project
is explicitly untrusted. Setting it trusted in kah's overlay skips the dialog but
enables repo-local configuration; kah retains untrusted project settings instead.
This is an explicit startup limitation, not a claim of zero-touch launch.

The subscription login stays on the host. Commands execute in gVisor even for
Mine-source runs. Codex file tools use its executor; Claude's native project file
tools use invocation-local permission rules, including protection of the trusted
candidate and private Git metadata. Global setup and native subagents remain
available. Workspace egress is offline by default; `--online` enables it. Hosted
search and configured online MCP executors retain the interactive policy. The old
`--allow-host` fetch broker is superseded.

Git configuration, hooks and directory pointers are read-only in the worker;
normal indexes, commits and branches remain native. The controller checkpoints
inside gVisor and imports a checked bundle into a separate trusted candidate.
It exports only the local `kah/run-ID` branch into the source repository. kah does
not check out, merge, push or deploy that branch; the user reviews and merges it.

Each verification uses a fresh checkout of that candidate, outside the worker's
mount, with Git metadata read-only. Ignored worker files cannot satisfy it. Use
the project's normal verification entrypoint so steering can update its tests
without changing the host command. Completion requires unchanged candidate
verification, current user sequence/checkpoint matching, and two independent Jev
judgments satisfying the recorded outcomes. The existing 0.90 probability and
0.85 confidence thresholds remain. Agent claims alone are not completion.

The completion questions remain focused on delivered project code. An early
infrastructure-only fixture conservatively stopped despite passing file checks;
that rejected evidence is retained, not converted into a passing gate or used
to tune thresholds. Native code-change journeys pass. Reports preserve separate
assessment receipts, verification and the resulting state.

`--budget-tokens N` counts reported native and observer tokens, including cached
input. Native usage can arrive only after work has consumed it, and interrupted
calls may lack a final report; these counters are not billing totals. Time and
token counters survive resume. Exhausted allowances are not reset. kah never
resets provider usage or requests a reset.

A killed controller's native child receives a parent-death signal. Resume uses
the saved native session and preserves private worktree edits; it does not reset
the worktree to an older checkpoint. The separate run lease prevents concurrent
controllers. Old ephemeral-worker runs are inspectable but cannot be resumed as
a native session; start a new run from their reviewed branch.

Reports live at `~/.local/state/kah/runs/run-ID/report.json`. Review the local branch
diff and verification before merging. Live stop/resume, cancellation and descendant
usage coverage remain required work before Phase 5R is complete.
