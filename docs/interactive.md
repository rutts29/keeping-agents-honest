# Interactive launcher

`make install` builds the release launcher and installs it as `~/.local/bin/kah`,
with its shell helpers in `~/.local/share/kah/shell.sh` and `~/.local/bin/cx`.

`make install` does not install or configure rootless Docker, gVisor, a sandbox
service account, the sandbox image, the native agent CLIs, Herdr or logins. Those
belong to the author's server setup and are not packaged yet. No user setup
guide exists yet.

```sh
kah                              # current project, native Codex
kah --agent claude
kah research new-project         # create a Git repository with uncommitted guidance files
kah research project --online    # enable workspace egress
kah untrusted project --online
kah mine project --yolo          # native full host access, Mine only
kah --model MODEL --effort EFFORT
kah -- resume --all              # native picker, no kah combined view
kah --agent claude -- --resume SESSION_ID
```

Model and effort remain native choices; kah supplies no default override. Mine
uses host workspace permissions. Research and Untrusted keep the controller and
subscription login on the host, with commands in rootless gVisor. Codex native
file tools also use its executor. Claude uses native project file tools with
permission denies on sensitive host paths; its Bash uses the native shell prefix
(see [Claude execution](claude-adapter.md)).
Workspace egress is offline unless `--online` is supplied in either restricted
domain. Hosted search stays on. Claude WebFetch follows the online choice.

Restricted launches share selected installed setup paths and apply the configured
hook and MCP policy: the user's global guidance, skills, plugins, hooks and MCP
setup remain available, mounted directly rather than copied into frozen snapshots.
Stdio MCP servers use an online executor by default; a kah config list can select
an offline executor. Declare secret-bearing servers in `~/.config/kah/setup.json`
and enable each once; see [setup policy](setup.md). Remote MCP connections
remain native host-controller connections. kah observer hooks stay host-side.
User-owned Claude exec-form hooks can run on the host.
Research/Untrusted exclude repository executable Claude settings through native
`--setting-sources user`; project instructions, skills and agents remain usable.
Codex marks restricted projects untrusted in its independent overlay.

Restricted Git config, hooks and directory pointers are read-only inside gVisor.
Claude native file tools also deny Git administration edits. Change remotes and
Git configuration from your host shell. Normal commits, index updates and branches
still work. Inherited ACLs keep sandbox-created files editable by the user.

Installed Bash routing sends interactive `codex`, resume and fork through kah under
the three domain roots. Recognized native management commands pass through; the
classifier reads installed help with a version cache and a static fallback.
`command codex` bypasses this convenience function. `cx` forwards to `kah`.

Codex overlays are stable per domain/network policy and project under
`~/.local/state/kah/codex`. They share the native login and history stores while
keeping independent writable config. Keep overlay paths: saved rollouts reference
them. Claude uses its configured native store, which defaults to `~/.claude`, with
project-keyed history and invocation-local settings. No kah cross-domain picker or credential copy
exists. Native `--all` passes through. Global provider configuration is unchanged.

Codex resume by ID restores its saved project and online/yolo policy. In-process
`/resume` must match the current project and policy; otherwise kah blocks prompts
with the command to reopen correctly. This guard remains with `--no-observer`.
For Claude, start in the saved project when resuming.

The observer reads authoritative native user records and retains their exact
words. In chat, `steer:` records an instruction and `aside:` marks commentary.
Leading `!` is native shell syntax, never a kah override. Jev supplies typed
judgments, and native observer sessions prepare whole-record evidence when
needed. `kah status
[SESSION_ID]` shows gaps/usage; `kah ledger [SESSION_ID]` shows source-linked intent.
Observer failures are recorded as gaps or service failures and can block review
or completion; the native session stays usable.

With the observer attached, kah names a new session after the first prompt/turn
using its project, native ID and prompt. Claude `--name`, existing resume names
and later user renames are preserved. Names are cosmetic. `--no-observer` disables
the hooks supplying automatic names.

kah launches Codex with `--no-daemon` and launches Claude as a native interactive
process. A session without a Codex control socket can receive messages through
its recorded Herdr pane. Unattended run-ID controls work through their own Herdr
panes. Shared history does not provide remote live control; see
[startup modes](startup-modes.md) for the transport boundaries.
