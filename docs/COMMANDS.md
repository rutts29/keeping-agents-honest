# Daily commands

| Domain | Interactive execution | Network | Extra option |
|---|---|---|---|
| Mine (`~/repos/mine`) | Host workspace permissions | Enabled | `--yolo` for native full host access |
| Research (`~/repos/research`) | gVisor, project only | Sandbox offline by default; hosted search on | `--online` enables sandbox egress |
| Untrusted (`~/repos/untrusted`) | gVisor, project only | Sandbox offline by default; hosted search on | `--online` enables sandbox egress; no yolo |

```sh
kah                                      # current project
kah mine new-project                     # create project if absent
kah research project --online
kah untrusted project
kah --model MODEL --effort EFFORT          # optional native choices
kah -- resume --all                       # shared native history picker
kah -- resume SESSION_ID                  # restores saved project and policy
```

Plain `codex` in these roots routes through kah via `.bashrc`; `cx` forwards to kah.
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
executable settings are excluded. Native Claude send/steer transport is not
implemented; type overrides in chat or use Herdr. Unattended run controls use
their own Herdr pane.

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

Native send/steer needs an initialized live conversation with a control socket.
Embedded kah TUIs currently lack that socket; type steer:/aside: in their chat. Reuse the same receipt
and exact text on retry; use a new receipt for each new message. `kah send` marks
delegate provenance and cannot create user intent. Review commands require user
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
workspace egress. kah returns a local `kah/run-ID` branch for user review/merge.
Native launch, completion, steering, kill/resume and limits pass live; see STATUS.

`kah smoke` runs one ephemeral native Codex command in a disposable offline
Research executor, checks host isolation and cleans up. It uses your configured
model/login and incurs one short model turn. Automation may use this smoke;
ordinary development must not invoke the updater or sudo.

Completion diagnostics: `kah eval completion ~/.local/share/kah-eval/completion/FILE.json`
replays the saved scrubbed Jev request without changing an active run or its
policy. Captures are private; copying them elsewhere is a separate user choice.
