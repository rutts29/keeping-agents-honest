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

The native launch, controls and resume gates pass for both providers; see [status](STATUS.md).
The default agent is Codex. Model and effort are optional native choices. The
observer's compactor and Jev alias use the separate `--observer-model`,
`--observer-effort`, and `--jev-model` options.

A run starts at committed HEAD in a private worktree and its own Herdr pane.
New-project initialization leaves its guidance files uncommitted. The user or
native agent makes the source repository's first commit before starting a run.
Uncommitted source edits are not imported. The task is the first native prompt;
there is no exec/print worker or required JSON reply. Native consent and prompts
remain native. Codex Mine-source runs mark their private path trusted in kah's
own overlay, matching the user's trust in the source repository. Codex restricted
sources remain explicitly untrusted; Claude private repositories also need native
startup consent before work can proceed. kah shows a Herdr notification
when the agent is blocked, and a startup notice after 15 seconds without the first
task submission. `kah status` records the wait and the native pane ID. Attach to
that pane to answer; kah never sends consent keys. The wait clears after submission.
Logs show that pane's recent output. Run time limits continue during this wait.
Logs use the run's recorded Herdr server, even when requested from another server.
Notifications honor Herdr's configured `[ui.toast]` delivery, including `off`;
kah also prints the consent notice at launch and retains the wait in status.

Claude 2.1.286 has no documented invocation-local interactive trust flag; print
mode skips trust but would change the required native session flow. Its trust
acceptance is project state in ~/.claude.json, not a --settings property. kah does
not edit that file. Codex 0.159.0 also asks for consent when an embedded project
is explicitly untrusted. Setting it trusted in kah's overlay skips the dialog but
enables repo-local configuration; kah retains untrusted settings for restricted
sources. Claude inherits parent trust only until a nested Git repository, so
trusting the stable run parent does not cover these separate private repositories.
This is an explicit startup limitation, not a claim of zero-touch launch.

The subscription login stays on the host. Commands execute in gVisor even for
Mine-source runs. Codex file tools use its executor; Claude's native project file
tools use invocation-local permission rules, including protection of the trusted
candidate and private Git metadata. Global setup and native subagents remain
available. Workspace egress is offline by default; `--online` enables it. A task
or `--test` command that fetches dependencies (for example Cargo crates) needs
`--online`; verification uses the same network setting. Hosted
search and configured online MCP executors retain the interactive policy. The old
`--allow-host` fetch broker is superseded.

Each new run requests a unique native name derived from its run ID and task.
Claude receives `--name`; Codex uses its native `thread/name/set` API when the
thread becomes available. Real resumes preserve the name and native model/effort
selection. Names are labels, not authority. Claude's own host SessionStart hook
and transcript bind the run; Herdr's PID-file-derived session ID is advisory.

Git configuration, hooks and directory pointers are read-only in the worker;
normal indexes, commits and branches remain native. The agent commits the work
it intends to deliver. kah never stages files or authors commits, including seed
commits or verification-side-effect commits. It reads the agent's HEAD in gVisor
and imports a fsck-checked bundle into a separate trusted candidate.
It exports only the local `kah/run-ID` branch into the source repository. kah does
not check out, merge, push or deploy that branch; the user reviews and merges it.
Agent amendments are reflected only in that run's local delivery branch.

Keep confidential, personal and internal material out of commits. Honor
`.gitignore` and `.git/info/exclude`; an ignored local-only folder such as `notes/`
can remain in the worker tree. kah preserves local exclude rules and the configured
Git identity in the private repository. It does not decide which files to stage.
Only committed content is delivered; uncommitted leftovers are not copied into
the candidate. Necessary code left uncommitted fails verification/assessment of
the delivered SHA, and the run continues or asks for steering. A global prohibition
on committing required code conflicts with delivery and needs the user's decision.

Each verification uses a fresh checkout of that exact HEAD SHA, outside the worker's
mount, with Git metadata read-only. Ignored worker files cannot satisfy it. Use
the project's normal verification entrypoint so steering can update its tests
without changing the host command. Completion requires unchanged candidate
verification, current user sequence and HEAD matching, and two independent Jev
judgments satisfying the recorded outcomes. The existing 0.90 probability and
0.85 confidence thresholds remain. Agent claims alone are not completion.

The completion questions remain focused on delivered project code. An early
infrastructure-only fixture conservatively stopped despite passing file checks;
that rejected evidence is retained, not converted into a passing gate or used
to tune thresholds. Native code-change journeys pass. Reports preserve separate
assessment receipts, verification and the resulting state.
Commit evidence includes reachable changes across the delivered range, so adding
private notes and deleting them in a later commit does not hide them from the judge.
Reports name the delivered commit and only attach tests that verified that SHA.

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

If killed before any primary user prompt was durably accepted, the native session
may not exist in provider history. kah then starts a fresh native session with the
original task and keeps the run's steering and counters. Previously acknowledged
or metered history that disappears is an error; kah will not silently replace it.
An exhausted run is refused before allocating another pane. Stop/steering/time
changes interrupt verification; the guest also has its own test deadline (600 s).

The report is fsynced and atomically renamed before public status becomes terminal.
While shutdown/report publication is pending, `kah status` reports Stopping. Native
transcripts are never deleted by product cleanup. Keep the session ID and name
shown in status/report when removing a finished worktree. Claude history survives
that removal and can be opened from the original source repository with
`claude --resume SESSION_ID`; this was tested without sending another model prompt.
Codex likewise reopened its saved history with `codex --no-daemon --cd SOURCE
resume SESSION_ID`, using the run's kah-owned `inspection` directory as CODEX_HOME.
That profile shares native login/history and is for history inspection; the test
sent no model prompt and did not claim continued tool execution after cleanup.
Continuing a kah-controlled run still requires its worktree. Native history viewing
after cleanup is distinct from restoring the deleted working files.

Reports live at `~/.local/state/kah/runs/run-ID/report.json`. Review the local branch
diff and verification before merging. Native stop/resume, cancellation and descendant
usage journeys are recorded in the accepted Phase 5R gates.
