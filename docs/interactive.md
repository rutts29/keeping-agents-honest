# Interactive launcher

`make install` installs the Rust launcher as `~/.local/bin/kah`. Use a release
build: validating the native executable is noticeably slower in debug builds.

```sh
kah                              # current project
kah research new-project         # creates and commits guidance if absent
kah research project --online    # explicitly enables project egress and search
kah mine project --yolo          # native full access, Mine only
kah --model gpt-6-sol --effort low
kah -- resume --all
```

Model and effort remain native choices; kah has no default override for either.
Mine uses a native workspace permission profile with network and automatic
approval review. Research and Untrusted use a rootless gVisor executor with only
the selected project mounted. Untrusted is always offline. Restricted domains
exclude host MCP, user/project hooks and plugins/apps. Some native flags that
would change those boundaries are deliberately refused with a reason code.

Optional shell routing: source `~/.local/share/kah/shell.sh` in Bash. Plain
`codex` then routes through kah under the three domain roots. This is a convenience,
not a security boundary; `command codex` bypasses it.

Overlays are stable per domain/network policy and project under
`~/.local/state/kah/codex`. They share native login and session/history stores,
while keeping a separate writable config. Keep overlay paths: saved native
rollouts reference them. No user global config is rewritten by a launch.
Resuming by native session ID resolves its saved project before launch, including
its prior explicit Research-online or Mine-yolo choice. This also handles Herdr
restoring a native session from the original shell directory. Native session
pickers still share the same history; use the displayed session ID to reopen a
session in another project through kah.

Inside Herdr, restricted sessions use one kah-owned SessionStart hook for native
session identity. kah obtains that declaration's trust hash from Codex and trusts
only that exact hook in the independent overlay. User/project hooks remain
excluded. Herdr's existing screen detection tracks the native Codex lifecycle.
The native UI calls its internal executor permissions “YOLO mode”; gVisor supplies
the external Research/Untrusted boundary.

The observer and unattended commands are still being implemented. Gate 2 passed;
see `STATUS.md` for journeys passed and outstanding.

kah fixes the launch directory for in-process `/resume`. The saved kah project
and policy must match, including online/yolo choices; otherwise the session and
subsequent prompts are blocked with a command to reopen through kah. Use
`kah -- resume SESSION_ID` to let the launcher select the saved project first.
This guard remains enabled with `--no-observer`.

The observer defaults to Luna/low; `--observer-model` and `--observer-effort`
configure it without changing the main model. `kah status [SESSION_ID]` shows
observer gaps and measured usage. `kah ledger [SESSION_ID]` reads the canonical
JSONL history/projection; `--json` exposes their structured form. Observer
failures leave native work usable and are visible as gaps. Restricted native
tools can read immutable snapshots of installed `~/.agents/skills` trees; their
scripts run inside gVisor. Restricted sessions continue to exclude user hooks, MCP, apps and plugins.

`--observer-judge jev|luna` selects the typed judge for this launch. Mine defaults
to Jev with Luna fallback; Research and Untrusted default to Luna. Selecting Jev
explicitly in a restricted domain opts that session into sending bounded question
context to TypeSafe. `--jev-model` chooses its model/alias; the default is
`jev-latest`. Native text extraction still uses `--observer-model` and
`--observer-effort`; judgments record the actual backend and returned model.
