# Interactive launcher

`make install` installs the Rust launcher as `~/.local/bin/kah`. Use a release
build: executable validation is noticeably slower in debug builds.

```sh
kah                              # current project, native Codex
kah --agent claude
kah research new-project         # create and commit guidance if absent
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
permission denies on sensitive host paths; its Bash uses the native shell prefix.
Workspace egress is offline unless `--online` is supplied in either restricted
domain. Hosted search stays on. Claude WebFetch follows the online choice.

The user's global guidance, skills, plugins, hooks and MCP setup remain available.
Installed setup is mounted directly rather than copied into frozen snapshots.
Stdio MCP servers use an online executor by default; a kah config list can select
an offline executor. Declare secret-bearing servers in `~/.config/kah/setup.json`
and enable each once; see [setup policy](setup.md). Remote MCP connections
remain native host-controller connections. kah observer hooks stay host-side.
User-owned Claude exec-form hooks may also remain on the host (accepted limit).
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
them. Claude uses the single native `~/.claude` store and project-keyed history,
with invocation-local settings. No kah cross-domain picker or credential copy
exists. Native `--all` passes through. Global provider configuration is unchanged.

Codex resume by ID restores its saved project and online/yolo policy. In-process
`/resume` must match the current project and policy; otherwise kah blocks prompts
with the command to reopen correctly. This guard remains with `--no-observer`.
For Claude, start in the saved project when resuming.

The observer reads authoritative native user records and retains their exact
words. In chat, `steer:` records an instruction and `aside:` marks commentary.
Leading `!` is native shell syntax, never a kah override. Jev is the only judge;
only oversized intent compaction uses the configured text model. `kah status
[SESSION_ID]` shows gaps/usage; `kah ledger [SESSION_ID]` shows source-linked intent.
Observer failures remain visible and leave the native session usable.

With the observer attached, kah names a new session after the first prompt/turn
using its project, native ID and prompt. Claude `--name`, existing resume names
and later user renames are preserved. Names are cosmetic. `--no-observer` disables
the hooks supplying automatic names.

All kah sessions use embedded native mode. There is no live global-daemon control
socket for `kah send` or native-session `kah steer`; use chat or Herdr. Unattended
run-ID controls work through their own Herdr panes. Saved history visibility does
not imply live control from Mac/phone apps. See [startup modes](startup-modes.md)
and [status](STATUS.md) for accepted limits.
