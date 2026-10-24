# Native startup and session transport

How kah starts native Codex and how messages reach a running session. Read it
when a launch rejects your options or `kah steer`/`kah send` cannot reach a
session.

## How Codex is started

kah starts Codex as an embedded session in every
[domain](../README.md#domains): `--no-daemon`, with the TUI and app server in
one process rather than attached to a shared Codex daemon. Each session uses
the launch policy in its project overlay, the per-project Codex configuration
kah writes under its state root.

| Domain | Why embedded |
|---|---|
| Mine | The session gets kah's overlay and hooks. |
| Research, offline or online | A shared daemon has the host's global tools, extensions and config; it cannot provide the restricted overlay and gVisor environment. |
| Untrusted | The same separation, offline unless `--online` is given. |

Every Codex launch refuses `--remote` (and `--remote=...`), because a remote
app server cannot adopt the launch's isolated configuration. Claude Code
launches do not check for this flag.

## Model and effort

kah passes `--model` to Codex only when you supply it (`--model MODEL`,
`-m MODEL` or `--model=MODEL`). The same model also configures kah's internal
app-server probes, which have no model flag of their own. An explicit
`--effort` is written to the overlay as `model_reasoning_effort`; without it,
your configured value stays. kah never invents or pins a model or effort.
Native configuration flags you pass yourself are your choices, not kah policy.

## Saved history and live control

Saved history and live control are separate:

- kah overlays share the native history stores.
- A saved session record does not mean a control endpoint owns that session.
  Embedded sessions are not threads of a shared daemon, and the embedded
  interface has no overlay control socket.
- For user steering, and for sessions with a recorded kah endpoint, kah uses a
  control socket only after it checks that it owns the target session. A
  delegate `kah send` to a session without a recorded endpoint goes to the
  socket without that check.
- Shared history does not let an external app control a kah session. kah has
  no remote-device control interface.

The shell wrapper passes `codex agents` through as a native management
command. kah refuses `agents` as a project launch, so it cannot start or
select a daemon by accident. Source `~/.local/share/kah/shell.sh` in the
current shell to turn on this routing.

## How messages reach a live session

- **Native socket.** Used for user steering and for sessions with a recorded
  kah endpoint, after kah's ownership check. A delegate `kah send` without a
  recorded endpoint skips that check.
- **Herdr pane.** Used when socket delivery is unavailable. It needs the pane
  and server that kah's host hook recorded for the session.
- **Neither route valid.** kah reports a delivery error.
- **No route at all.** Type `steer:` or `aside:` in the session's chat.

Use the native session ID with `kah send` or `kah steer`. Steering an
unattended run by its run ID uses the run's own Herdr pane.

## Trust and testing changes

Native clients can keep trust state outside kah's overlay. kah applies its
provider settings only through its overlay or invocation-local configuration,
and does not configure shared-daemon policy or hooks for restricted launches.
To try startup changes by hand, use a disposable native home: set an isolated
`CODEX_HOME` for daemon experiments.

## Hooks

Each overlay gets one regular `hooks.json`, rebuilt at every launch:

- In Mine, it merges your global JSON and TOML hook declarations with kah's
  hooks.
- In restricted domains, your hooks are kept and their shell commands run
  through gVisor. kah's observer hooks stay on the host.
- Hook trust state lives in the `hooks.state` table of the overlay's
  `config.toml`, remapped for the new positions and checked against native
  hashes. Changed hooks are not silently trusted, disabled hooks stay
  disabled, and kah newly trusts only its own hooks.
- Your global config and hooks files are never written.

## Resuming in the right project

Restricted launches pass an explicit `--cd` for the selected project or
subdirectory. Codex needs this when it resumes a remote execution workspace
with `tui.resume_cwd=current`; changing only the process directory is not
enough.

The session-policy guard stays on. If the saved project or domain policy
differs from the current launch, kah's hook stops prompts and tool calls with
`SESSION_POLICY_MISMATCH` and names the command that reopens the session
correctly.

## References

These links point to Codex `rust-v0.159.0` source.

- [Daemon exclusion and compatibility rules](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/tui/src/daemon_startup.rs)
- [Explicit no-daemon startup](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/tui/src/startup_orchestration.rs)
- [Available native model/configuration flags](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/utils/cli/src/shared_options.rs)
- [Per-thread TUI request configuration](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/tui/src/app_server_session.rs)
- [Daemon home and endpoint selection](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/app-server-daemon/src/lib.rs)
- [Protected socket alias and hash](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/app-server-transport/src/transport/unix_socket.rs)
- [In-process TUI transport](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/app-server/src/in_process.rs)
- [Hook environment and discovery](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/hooks/src/registry.rs)
