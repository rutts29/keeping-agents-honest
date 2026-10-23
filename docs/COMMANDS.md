# Daily commands

| Domain | Interactive execution | Network | Extra option |
|---|---|---|---|
| Mine (`~/repos/mine`) | Host workspace permissions | Enabled | `--yolo` for native full host access |
| Research (`~/repos/research`) | Restricted: gVisor commands, project and selected setup mounts | Sandbox offline by default; hosted search on | `--online` enables sandbox egress |
| Untrusted (`~/repos/untrusted`) | Restricted: gVisor commands, project and selected setup mounts | Sandbox offline by default; hosted search on | `--online` enables sandbox egress; no yolo |

Restricted commands run in gVisor. Claude's native file tools remain on the host
under invocation-local permissions; see [Claude execution](claude-adapter.md).

```sh
kah                                      # current project
kah mine new-project                     # create project if absent
kah research project --online
kah untrusted project
kah --model MODEL --effort EFFORT          # optional native choices
kah -- resume --all                       # shared native history picker
kah -- resume SESSION_ID                  # restores saved project and policy
```

After you source `~/.local/share/kah/shell.sh`, interactive `codex` commands under
the three domain roots route through kah; `cx` forwards to kah.
Native Codex management commands pass through, including app-server, login,
update and features. Existing shells can refresh the function with
`source ~/.local/share/kah/shell.sh`.
`command codex` bypasses the convenience function. In-TUI `/resume` must match
the current project/policy; use `kah -- resume SESSION_ID` across projects.

Claude Code is available in all three domains:

```sh
kah --agent claude
kah --agent claude --model MODEL --effort EFFORT
kah --agent claude -- --resume SESSION_ID
```

Start in the saved project when resuming. Omitted model and effort use native
choices. kah adds invocation-local hooks; global settings/hooks stay intact.
Restricted launches use user settings plus invocation-local policy; repository
executable settings are excluded. kah can deliver Claude session
messages through the Herdr pane recorded by its host hook. Unattended run
controls use their own Herdr pane.

Type ordinary chat messages beginning `steer: Keep the API unchanged` or
`aside: This is only a note`. Case and leading whitespace do not matter. `steer:`
forces recording; `aside:` excludes steering inference. Leading `!` is native
shell syntax, not a kah override. Latest explicit user instructions take precedence.

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

Session messaging uses a connected Codex control endpoint or a validated
recorded Herdr pane. An embedded session can receive CLI messages through Herdr
when kah has recorded its pane and server; otherwise type `steer:`/`aside:` in its
chat. Reuse the same receipt and exact text on retry; use a new receipt for each
new message. `kah send` marks
delegate provenance and cannot create user intent: its sender must be `kah-`
followed by lowercase letters, digits or `-` (default `kah-builder`; `kah-user`
is reserved), so the `[sender]` prefix it adds is read as delegate context. Review commands require user
authority and call no judge. Add `--json` to status/ledger for structured output.

```sh
kah run --agent claude --test 'make check' --time-limit 600 --budget-tokens 100000 'Implement the change'
kah steer run-ID 'Use the revised behavior'
kah status run-ID
kah logs run-ID
kah stop run-ID
kah resume run-ID
```

Start unattended work inside Herdr. It opens a full native session on a private
worktree from committed HEAD, with gVisor commands in every domain and model
logins on the host. Choose `--model`/`--effort` if desired; `--online` opens
workspace egress, which a task or test that fetches crates or packages needs.
kah returns a local `kah/run-ID` branch for user review/merge.
See [unattended work](unattended.md) for run controls, verification and resume
behavior.

`kah smoke` runs one ephemeral native Codex command in a disposable offline
Research executor, checks host isolation and cleans up. It uses your configured
model/login and incurs one short model turn. Run it explicitly when you want to
check the configured native launch and sandbox.

`kah eval replay DATASET.jsonl [--validation]` evaluates a labelled message
dataset. Matching cached results are reused; uncached eligible user rows are
sent to the live Jev API, requiring a TypeSafe key and incurring API usage.
Policy and thresholds are unchanged.

Completion diagnostics: `kah eval completion ~/.local/share/kah-eval/completion/FILE.json`
resends the saved scrubbed Jev request to the live Jev API, which requires a
TypeSafe key and incurs API usage, and writes the new result as a private file.
It does not change an active run or its policy. Captures are private; copying
them elsewhere is a separate user choice.

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

Launch options (`OPTIONS`):

- `--agent codex|claude` (default `codex`).
- `--online` (Research and Untrusted workspace egress), `--yolo` (Mine only).
- `--model MODEL` (also `-m MODEL` or `--model=MODEL`) and `--effort EFFORT`:
  native choices.
- `--no-observer`, `--observer-model MODEL`, `--observer-effort EFFORT`,
  `--observer-judge jev` (the only judge), `--jev-model MODEL`.
- `--` passes the rest to the native client. The first argument that is not an
  kah option also starts the native arguments.

Codex native arguments are checked before launch. Every domain refuses
`--remote`, and refuses the native subcommands `exec`, `review`, `app-server`,
`exec-server`, `sandbox`, `mcp`, `plugin`, `agents`, `remote-control`, `apply`
and `cloud` (use `kah run` for unattended work). Research and Untrusted also
refuse any other native flag except `--model`, `--model=`, `-m`, `--image`, `-i`,
`--search`, `--all`, `--last`, `--no-alt-screen`, `--no-daemon`, `--help`, and
`-c`/`--config` with only `model_reasoning_effort` or `service_tier`.

Run options (`RUN_OPTIONS`):

- `--agent codex|claude`, `--model MODEL`, `--effort EFFORT`, `--online`.
- `-c KEY=VALUE` or `--config KEY=VALUE`: only `model_reasoning_effort` and
  `service_tier`, and only for Codex runs (`RUN_CONFIG_PROVIDER` with
  `--agent claude`).
- `--observer-model MODEL`, `--observer-effort EFFORT`, `--jev-model MODEL`;
  see [unattended work](unattended.md) for the defaults.
- `--budget-tokens N`, `--time-limit SECONDS` (positive integers).
- `--domain mine|research|untrusted`: record the run under a more restrictive
  domain than the project's directory; a less restrictive one is refused
  (`RUN_DOMAIN_POLICY`).
- `--parent SESSION`: the dispatching session, normally inferred in a host
  agent pane.

Names that start with `__`, and `--sandbox-shell` and `--sandbox-exec`, are
internal entrypoints for kah's hooks, background jobs and sandbox commands. They
are not user commands.
