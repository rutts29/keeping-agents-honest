# Unattended work

`kah run` gives a task to a native agent session in a private worktree,
verifies the agent's commit and returns a local branch for review. This page
covers run options, the delivery and completion rules, limits and resume.

## Quick start

Start inside Herdr, from a project under
`~/repos/{mine,research,untrusted}` (see [domains](../README.md#domains)):

```sh
kah run --agent claude --test 'make check' 'Implement the requested change'
kah run --model MODEL --effort EFFORT --time-limit 600 --test 'make check' 'Implement the change'
kah steer run-ID 'steer: Use the revised behavior instead'
kah status run-ID
kah logs run-ID
kah stop run-ID
kah resume run-ID
```

The project must be an ordinary Git repository (a `.git` directory, no
alternates object store) with at least one commit. A new project leaves its
guidance files uncommitted, so you or the agent make the first commit before
starting a run. Uncommitted edits in your repository are not copied into the
run.

## Agent, model and observer

- The agent is Codex by default; `--agent claude` selects Claude Code. Both run
  as full native interactive sessions. The task is the agent's first prompt.
- `--model` and `--effort` are optional native choices for the agent.
- `--observer-model` and `--observer-effort` set the observer. The defaults are
  `gpt-6.1-sol` for a Codex run or `claude-sonnet-5-5` for a Claude run, with
  effort `low`. The observer model must match the run's provider (a `claude-`
  model exactly when the agent is Claude); otherwise the run is refused with
  `OBSERVER_PROVIDER_MODEL`.
- `--jev-model` selects the Jev model (default `jev-latest`).

## What happens during a run

1. kah creates a private Git worktree from your committed HEAD and opens the
   agent in its own Herdr pane.
2. The agent works and commits what it intends to deliver. kah never stages
   files or authors commits, including seed commits or commits caused by
   verification.
3. kah reads the agent's HEAD and imports it, as a checked Git bundle, into a
   separate repository that only kah writes (the candidate).
4. kah creates or updates a local `kah/run-ID` branch in your repository with
   that commit. kah does not check out, merge, push or deploy it; you review
   and merge it.
5. kah verifies that exact commit in a fresh checkout and runs the observer's
   completion checks.

The branch exists as soon as the commit is captured, so its existence does not
mean those checks passed. Check the run report and status before merging.

If the agent amends its commits, only the run's own delivery branch changes.

## Startup prompts and consent

A fresh private repository may need the agent's native startup consent. kah
does not answer these prompts and does not edit the provider's global trust
store to skip them, so launch is not zero-touch.

- Claude runs start with `--permission-mode acceptEdits`, so Claude's file-edit
  prompts are accepted. Other native prompts stay native.
- For Codex runs from a Mine project, kah marks the private run path trusted in
  its Codex overlay (kah's per-project Codex configuration), matching your trust in the source repository. Runs from Research
  or Untrusted projects stay untrusted.
- When the agent is blocked, kah shows a Herdr notification. If the first task
  has not been submitted after 15 seconds, it also shows a startup notice. If
  Herdr toasts are off, the notice is still printed at launch and kept in
  status.
- `kah status` shows the wait and the agent's pane ID. Attach to that pane to
  answer. kah never sends consent keys. The wait clears once the task is
  submitted. The run's time limit keeps counting during the wait.

`kah logs` shows the pane's recent output, using the run's recorded Herdr
server even when you ask from another server.

## Sandbox and network

- Your model login stays on the host.
- The agent's commands run in gVisor in every domain,
  including runs from Mine projects.
- Codex file tools use the sandbox. Claude's native file tools use
  invocation-local permission rules, which also protect the candidate and the
  run's private Git metadata.
- Your global setup and the agent's native subagents remain available.
- Workspace network access is off by default. Use `--online` when the agent or
  the test command needs it, for example to fetch Cargo crates. Verification
  uses the same network setting. Hosted search and online MCP servers keep the
  interactive policy.

## Git rules inside the run

- Git configuration, hooks and directory pointers are read-only for the agent.
  Normal indexes, commits and branches work.
- kah keeps your local exclude rules and configured Git identity in the
  private repository. It does not decide which files to commit.
- Keep confidential, personal and internal material out of commits. Files
  ignored by `.gitignore` or `.git/info/exclude`, such as a local `notes/`
  folder, can stay in the agent's tree.
- Only committed content is delivered. Uncommitted files are not copied, and
  they cannot satisfy verification or completion checks; the run continues or
  asks for steering.
- A global rule that forbids committing required code conflicts with delivery
  and needs your decision.

## Verification and completion

Each verification uses a fresh checkout of the exact HEAD SHA, outside the
agent's workspace, with Git metadata read-only. Ignored files in the agent's
tree cannot satisfy it. Use the project's normal test entrypoint, so steering
can change the tests without changing the command.

A run completes only when all of these hold:

- verification of the candidate passed and is unchanged;
- the current user messages and HEAD match what was checked;
- the observer's completion checkpoint passed for that delivery;
- two Jev completion judgments each support every required outcome, with
  probability at least 0.85 and confidence at least 0.80.

A run cannot complete while its delivery awaits a checkpoint. The agent saying
it is done is not completion, and a passing test command alone does not
complete a run.

The completion questions focus on the delivered project code. The evidence
names every path changed in every delivered commit, so a file added and later
deleted still appears by name. It also includes the net diff and the full
final text of the changed files. Content that existed only in intermediate
commits is not included. The report names the delivered commit and attaches
only tests that verified that SHA.

## Limits

- `--time-limit SECONDS` and `--budget-tokens N` set the run's allowances.
- The token count covers the agent session and its native subagents,
  including cached input, plus the Jev usage of the run's observer. A separate
  native observer review session records its tokens with that review; they do
  not count toward the budget.
- Native usage can be reported after the work used it, and an interrupted call
  may not report at all. These counters are not billing totals.
- Time and token counters survive resume. An exhausted allowance is not reset,
  and kah never resets or requests a reset of provider usage. An exhausted run
  is refused before kah opens another pane.
- Stopping, steering or a time-limit change interrupts a running verification.
  The test command also has its own 600-second deadline inside the sandbox.

## Stop and resume

- `kah stop` ends the run. While shutdown or the report is pending,
  `kah status` shows `Stopping`.
- `kah resume` continues the saved native session and keeps the agent's
  worktree edits; it does not reset the worktree to an older point. Only one
  controller can drive a run at a time.
- If the run's controller process is killed, the agent's native process
  receives a parent-death signal.
- If the controller was killed before the agent accepted its first prompt, the
  native session may not exist. kah then starts a fresh session with the
  original task and keeps the run's steering and counters. If history that the
  run already used disappears, kah reports an error instead of replacing it.
- Each new run gets a unique native session name from its run ID and task
  (Claude `--name`, Codex's native thread naming). Resume keeps the name, model
  and effort. Names are labels, not authority.

Cleanup keeps the native session transcripts. Keep the run's worktree while
kah controls the run; the run needs it to continue.

## Reports

Each run writes `runs/run-ID/report.json` in the kah state root
(`$XDG_STATE_HOME/kah` when that is absolute, otherwise `~/.local/state/kah`).
Before merging, review the delivered branch, the verification receipts and the
run's status.
