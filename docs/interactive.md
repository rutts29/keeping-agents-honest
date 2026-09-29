# Interactive launcher (Phase 2 in progress)

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

The observer and unattended commands are still being implemented. Gate 2 is
not yet accepted; see `STATUS.md` for journeys passed and outstanding.
