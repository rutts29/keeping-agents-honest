# Interactive sessions

Starting Claude Code or Codex under kah: what runs where in each domain, and
how resume, naming and messaging work.

## Install

`make install` builds the release launcher and installs it as
`~/.local/bin/kah`, with its shell helpers in `~/.local/share/kah/shell.sh`
and `~/.local/bin/cx`.

Rootless Docker, gVisor, the sandbox service account, the sandbox image, the
native agent CLIs, Herdr and logins are part of the host setup listed in the
[README requirements](../README.md#requirements). `make install` does not
install or configure them.

## Start a session

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

The project directory selects the domain (Mine, Research or Untrusted); see
[Domains](../README.md#domains).

## Models and the observer

Model and effort for the agent are native choices; kah supplies no default
override. The observer has its own settings:

- `--observer-model` defaults to `gpt-6.1-sol` for Codex and
  `claude-sonnet-5-5` for Claude. A Claude session needs a `claude-` observer
  model.
- `--observer-effort` defaults to `low`.
- `--jev-model` defaults to `jev-latest`.

## What runs where

- **Mine** uses host workspace permissions.
- **Research and Untrusted** keep the agent client and its subscription login
  on the host. Agent commands run in rootless gVisor. Codex's file tools also
  run through the sandbox. Claude Code
  keeps its native project file tools on the host, with permission rules that
  deny sensitive host paths, and its Bash commands run in the sandbox (see
  [Claude Code under kah](claude-adapter.md)).

Workspace egress is off unless you pass `--online` in either restricted
domain. Hosted search stays on. Claude's WebFetch follows the
`--online` choice.

## Your tools in Research and Untrusted

Your global guidance, skills, plugins, hooks and MCP setup stay available.
kah mounts the selected installed paths directly instead of copying them into
frozen snapshots.

- Local (stdio) MCP servers run in an online sandbox by default. A list in
  `~/.config/kah/setup.json` can run chosen servers offline.
- Declare servers that hold secrets in `~/.config/kah/setup.json` and enable
  each one once; see [setup policy](setup.md).
- Remote MCP connections stay native connections from the host.
- kah's observer hooks stay on the host. Your own Claude exec-form hooks can
  run on the host.
- Repository Claude settings that could run programs are excluded (native
  `--setting-sources user`); project instructions, skills and agents remain
  usable.
- Codex marks restricted projects untrusted in its kah overlay (see below).

## Git inside the sandbox

Git configuration, hooks and directory pointers are read-only inside the
sandbox, and Claude's file tools also deny edits to Git administration files.
Change remotes and Git configuration from your host shell. Normal commits,
index updates and branches still work. Inherited ACLs keep files created in
the sandbox editable by you.

## The `codex` shell function

After you source `~/.local/share/kah/shell.sh`, interactive `codex`, resume
and fork commands under the three domain roots go through kah.

- Recognized native management commands pass through. The function reads
  `codex --help` on each call to recognize them, with a built-in list when
  that fails.
- `command codex` bypasses the function.
- `cx` forwards to `kah`.

## Codex overlays and history

An overlay is a separate Codex configuration directory that kah keeps for each
domain, network policy and project. Overlays live under `codex/` in the kah
state root (`$XDG_STATE_HOME/kah` when that is absolute, otherwise
`~/.local/state/kah`).

- Overlays share your native login and history, but have their own writable
  configuration. Your global provider configuration is unchanged.
- Keep overlay paths: saved sessions refer to them. New overlays go under the
  state root. An overlay made in the default location before `XDG_STATE_HOME`
  was set is reused while no overlay exists under the active root, and a
  resumed session reuses the overlay of its saved policy, or the one its
  native session file names.
- Claude Code uses its configured native store, `~/.claude` by default, with
  project-keyed history and settings that apply to that launch only.
- kah has no cross-domain session picker and never copies credentials. Native
  `--all` passes through.

## Resuming

- Codex resume by ID restores the saved project and its online/yolo policy.
- Codex's in-session `/resume` must match the current project and policy.
  Otherwise kah blocks prompts and shows the command that reopens the session
  correctly. This guard stays on with `--no-observer`.
- For Claude Code, start in the saved project when resuming.

## The observer in a session

The observer reads the native user records and keeps your exact words. In
chat, `steer:` records an instruction and `aside:` marks commentary. A leading
`!` is native shell syntax, never a kah override.

- `kah status [SESSION_ID]` shows gaps and usage.
- `kah ledger [SESSION_ID]` shows source-linked intent.

Observer failures are recorded as gaps or service failures and can block
review or completion; the native session stays usable.

## Session names

With the observer attached, kah names a new session after its first prompt,
using the project, the native ID and the prompt. Claude `--name`, existing
resume names and later renames are kept. Names are cosmetic. `--no-observer`
turns off automatic names.

## Messaging a running session

kah launches Codex with `--no-daemon` and Claude Code as a native interactive
process. A session without a Codex control socket can receive messages through
the Herdr pane that kah recorded for it.
Unattended runs are controlled through their own Herdr panes. Shared history
does not give remote live control; see [startup modes](startup-modes.md).
