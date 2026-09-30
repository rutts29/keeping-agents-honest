# Daily commands

| Domain | Interactive execution | Network | Extra option |
|---|---|---|---|
| Mine (`~/repos/mine`) | Host workspace permissions | Enabled | `--yolo` for native full host access |
| Research (`~/repos/research`) | gVisor, project only | Offline by default | `--online` explicitly enables egress and hosted search |
| Untrusted (`~/repos/untrusted`) | gVisor, project only | Always offline | No online or yolo option |

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
`command codex` bypasses the convenience function. In-TUI `/resume` must match
the current project/policy; use `kah -- resume SESSION_ID` across projects.

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

Native send/steer needs an initialized live conversation. Reuse the same receipt
and exact text on retry; use a new receipt for each new message. `kah send` marks
delegate provenance and cannot create user intent. Review commands require user
authority and call no judge. Add `--json` to status/ledger for structured output.

```sh
kah run --test 'make check' --time-limit 600 --budget-tokens 100000 'Implement the change'
kah steer run-ID 'Use the revised behavior'
kah status run-ID
kah logs run-ID
kah stop run-ID
kah resume run-ID
```

Unattended work starts at committed HEAD and runs in gVisor in every domain.
Choose `--model`/`--effort` if desired. No push, deploy or credentials; review the
returned `kah/run-ID` branch and merge it yourself. Research may add
`--domain research --allow-host example.com` for bounded read-only HTTPS fetching.

`kah smoke` runs one ephemeral native Codex command in a disposable offline
Research executor, checks host isolation and cleans up. It uses your configured
model/login and incurs one short model turn. Automation may use this smoke;
ordinary development must not invoke the updater or sudo.
