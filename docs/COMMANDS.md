# Daily commands

The commands you use day to day, then the full reference of every command and
flag.

## Domains

The project directory selects the domain; see
[Domains](../README.md#domains).

| Domain | Interactive execution | Network | Extra option |
|---|---|---|---|
| Mine (`~/repos/mine`) | Host workspace permissions | Enabled | `--yolo` for native full host access |
| Research (`~/repos/research`) | Restricted: gVisor commands, project and selected setup mounts | Sandbox offline by default; hosted search on | `--online` enables sandbox egress |
| Untrusted (`~/repos/untrusted`) | Restricted: gVisor commands, project and selected setup mounts | Sandbox offline by default; hosted search on | `--online` enables sandbox egress; no yolo |

In restricted domains, Claude's native file tools stay on the host under
launch-only permissions; see [Claude Code under kah](claude-adapter.md).

## Launch

```sh
kah                                      # current project
kah mine new-project                     # create project if absent
kah research project --online
kah untrusted project
kah --model MODEL --effort EFFORT        # optional native choices
kah -- resume --all                      # shared native history picker
kah -- resume SESSION_ID                 # restores saved project and policy
```

Claude Code works in all three domains:

```sh
kah --agent claude
kah --agent claude --model MODEL --effort EFFORT
kah --agent claude -- --resume SESSION_ID
```

- Omitted model and effort use the native defaults.
- kah adds launch-only hooks; your global settings and hooks stay intact.
- Restricted launches use your user settings plus launch-only policy;
  repository executable settings are excluded.
- To resume Claude Code, start in the saved project.

## The `codex` shell function

After you source `~/.local/share/kah/shell.sh`, interactive `codex` commands
under the three domain roots go through kah, and `cx` forwards to kah.

- Native Codex management commands pass through, including `app-server`,
  `login`, `update` and `features`.
- `command codex` bypasses the function.
- Existing shells pick up changes when you source `shell.sh` again.
- In-session `/resume` must match the current project and policy; use
  `kah -- resume SESSION_ID` across projects.

## Steering and the ledger

Start a chat message with `steer:` to record it as an instruction, or `aside:`
to mark it as commentary. Case and leading whitespace do not matter. A leading
`!` is native shell syntax, not a kah override. Your latest explicit
instructions take precedence.

```sh
kah status [SESSION_ID]
kah ledger [SESSION_ID]
kah ledger SESSION_ID --review
kah ledger SESSION_ID --confirm ITEM_ID
kah ledger SESSION_ID --dismiss ITEM_ID
kah ledger SESSION_ID --promote ITEM_ID
kah steer SESSION_ID 'steer: Keep the API unchanged' --receipt change-001
kah send SESSION_ID '[kah-reviewer] Review findings' --sender kah-reviewer --receipt review-001
```

- `kah steer` and `kah send` deliver through the session's Codex control
  endpoint or the Herdr pane kah recorded for it. Without either, type
  `steer:` or `aside:` in the session's chat.
- A receipt is a message ID. Reuse the same receipt and exact text on retry;
  use a new receipt for each new message.
- `kah send` sends a delegate message: a message from a helper agent, not
  from you. It cannot create user intent. The sender must be `kah-` followed
  by lowercase letters, digits or `-` (default `kah-builder`; `kah-user` is
  reserved), and the `[sender]` prefix marks the message as delegate context.
- Ledger review commands are your own decisions and call no judge.
- Add `--json` to `status` or `ledger` for structured output.

See [user controls](user-controls.md) for details.

## Unattended runs

```sh
kah run --agent claude --test 'make check' --time-limit 600 --budget-tokens 100000 'Implement the change'
kah steer run-ID 'Use the revised behavior'
kah status run-ID
kah logs run-ID
kah stop run-ID
kah resume run-ID
```

Start runs inside Herdr. A run opens a full native session on a private
worktree from committed HEAD, with commands in gVisor in every domain and
model logins on the host. `--online` opens workspace egress, which a task or
test that fetches crates or packages needs. kah returns a local `kah/run-ID`
branch for you to review and merge. See [unattended work](unattended.md).

## Checks and evaluation

- `kah smoke` runs one short native Codex command in a disposable offline
  Research sandbox, checks host isolation and cleans up. It uses your
  configured model and login and costs one short model turn.
- `kah eval replay DATASET.jsonl [--validation]` evaluates a labelled message
  dataset. Matching cached results are reused; other eligible user rows go to
  the live Jev API, which needs a TypeSafe key and incurs API usage. Policy
  and thresholds are unchanged.
- `kah eval completion ~/.local/share/kah-eval/completion/FILE.json` resends a
  saved, scrubbed completion request to the live Jev API (TypeSafe key, API
  usage) and writes the new result as a private file. It does not change an
  active run or its policy.

## Command reference

Every command and flag that `kah` accepts:

| Command | Arguments and flags |
|---|---|
| `kah [OPTIONS] [mine\|research\|untrusted PROJECT] [NATIVE_ARGS]` | Launch a native session. Without a domain and project, the current directory selects them. |
| `kah status [SESSION\|RUN] [--json]` | Session and review status; without an ID, all sessions and runs. |
| `kah ledger [SESSION] [--json]` | Source-linked intent. |
| `kah ledger [SESSION] --review [--json]` | Items awaiting review and recent ignored sources. |
| `kah ledger SESSION --confirm\|--dismiss\|--promote ITEM_ID` | Record a user review decision. |
| `kah ledger SESSION --withdraw-link ITEM_ID LATER_SOURCE_ID` | Withdraw an active item, citing an existing later user source that withdraws it; neither message changes and no judge is called. |
| `kah steer SESSION TEXT [--receipt ID]` | Send user steering to a live session. |
| `kah steer SESSION TEXT --supersedes RECEIPT [--receipt ID]` | Steering that replaces an earlier steering receipt. |
| `kah steer SESSION TEXT --question ID --answer approve\|reject [--receipt ID]` | Answer an observer approval question. |
| `kah steer SESSION TEXT --question ID --choose NUMBER\|EXACT_OPTION [--receipt ID]` | Choose an option of an observer choice question. |
| `kah send SESSION TEXT [--sender NAME] [--receipt ID]` | Delegate message; see [user controls](user-controls.md) for sender and receipt rules. |
| `kah relay FROM_SESSION TO_SESSION MESSAGE_ID PARAPHRASE` | Relay an exact user message from one session to another. |
| `kah relay sources SESSION` | List the user messages of a session that can be relayed. |
| `kah run [RUN_OPTIONS] --test COMMAND TASK` | Start an unattended run, inside Herdr. |
| `kah steer RUN TEXT`, `kah status RUN [--json]`, `kah logs RUN`, `kah stop RUN`, `kah resume RUN` | Unattended run controls. |
| `kah smoke` | One ephemeral sandbox check. |
| `kah eval replay DATASET.jsonl [--validation]` | Evaluate a labelled message dataset. |
| `kah eval completion FILE.json` | Resend a captured completion request. |
| `kah --help`, `kah -h` | Usage. |

### Launch options (`OPTIONS`)

- `--agent codex|claude` (default `codex`).
- `--online` (Research and Untrusted workspace egress), `--yolo` (Mine only).
- `--model MODEL` (also `-m MODEL` or `--model=MODEL`) and `--effort EFFORT`:
  native choices.
- `--no-observer`, `--observer-model MODEL`, `--observer-effort EFFORT`,
  `--observer-judge jev` (the only judge), `--jev-model MODEL`.
- `--` passes the rest to the native client. The first argument that is not a
  kah option also starts the native arguments.

### Codex native argument checks

Every domain refuses `--remote`, and refuses the native subcommands `exec`,
`review`, `app-server`, `exec-server`, `sandbox`, `mcp`, `plugin`, `agents`,
`remote-control`, `apply` and `cloud` (use `kah run` for unattended work).
Research and Untrusted also refuse any other native flag except `--model`,
`--model=`, `-m`, `--image`, `-i`, `--search`, `--all`, `--last`,
`--no-alt-screen`, `--no-daemon`, `--help`, and `-c`/`--config` with only
`model_reasoning_effort` or `service_tier`.

### Run options (`RUN_OPTIONS`)

- `--agent codex|claude`, `--model MODEL`, `--effort EFFORT`, `--online`.
- `-c KEY=VALUE` or `--config KEY=VALUE`: only `model_reasoning_effort` and
  `service_tier`, and only for Codex runs (`RUN_CONFIG_PROVIDER` with
  `--agent claude`).
- `--observer-model MODEL`, `--observer-effort EFFORT`, `--jev-model MODEL`;
  see [unattended work](unattended.md) for the defaults.
- `--budget-tokens N`, `--time-limit SECONDS` (positive integers).
- `--domain mine|research|untrusted`: record the run under the same or a more
  restrictive domain than the project's directory; a less restrictive one is
  refused (`RUN_DOMAIN_POLICY`).
- `--parent SESSION`: the dispatching session, normally inferred in a host
  agent pane.

### Internal entrypoints

Names that start with `__`, and `--sandbox-shell` and `--sandbox-exec`, are
internal entrypoints for kah's hooks, background jobs and sandbox commands.
They are not user commands.
