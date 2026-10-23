# Native startup and session transport

kah launches embedded native Codex in every domain with `--no-daemon`.
The session uses the launch policy in its project overlay.

| Domain | Mode | Reason |
|---|---|---|
| Mine | Embedded | Mine launches use an embedded native session with kah's overlay and hooks. |
| Research offline/online | Embedded | A global daemon has global host tools, extensions and config; it cannot substitute for the restricted overlay and gVisor environment. |
| Untrusted | Embedded | Same separation, offline by default with explicit `--online`. |

Every Codex launch, in any domain, refuses `--remote` (and `--remote=...`):
a remote app server cannot adopt the launch's isolated configuration. Claude
Code launches do not check for this flag.

The model remains a native `--model` argument only when supplied. The explicit
model (`--model MODEL`, `-m MODEL` or `--model=MODEL`) also configures internal
stdio app-server probes, whose CLI has no model flag. An explicit kah `--effort` value is written to the Codex overlay as
`model_reasoning_effort`; omission preserves the user's configured value. No
model or effort is invented or pinned. Explicit user-supplied native
configuration flags are user choices, not kah-injected policy flags.

## Saved history and live control

kah applies the project overlay's configuration to the embedded session it
launches. Saved history and live session control are separate.

- kah overlays share the native history stores.
- A saved session record does not establish live ownership by a control
  endpoint. Embedded sessions are not loaded threads of a global daemon, and the
  embedded TUI transport is in-process with no overlay control socket.
- For user steering and for sessions with a recorded kah endpoint, kah uses a
  reachable control socket only after it checks that it owns the target session.
  A delegate `kah send` to a session without a recorded endpoint goes to the
  socket without that check.
- Shared history alone does not imply that an external app can control a kah
  session. kah does not provide a remote-device control interface.

The shell wrapper passes `codex agents` through as a native management command,
and kah refuses `agents` as an interactive project launch so it cannot implicitly
start or select a daemon. Source `~/.local/share/kah/shell.sh` in the current shell to enable
convenience routing.

## Delivery boundaries

- Native socket delivery of user steering, or to a session with a recorded kah
  endpoint, requires kah's ownership check of the requested live session. A
  delegate `kah send` without a recorded endpoint skips that check.
- Herdr delivery requires the pane and server recorded by kah's host hook.
- When native socket delivery is unavailable, `kah send` and native-session
  `kah steer` can use a validated recorded Herdr pane.
- If neither delivery route is valid, kah reports a delivery error.
- In-chat `steer:` and `aside:` remain available without an out-of-band delivery
  route.

Use the native session ID with `kah send` or `kah steer` when its host delivery
route is available. Unattended run-ID steering uses the run's own Herdr pane.

Native clients may maintain trust state outside kah's overlay. kah's provider
settings are applied through its overlay or invocation-local configuration.
Changes to native startup behavior should be checked in a disposable native
home: use an isolated `CODEX_HOME` for manual daemon experiments. kah does not
configure global-daemon policy or hooks for restricted launches. Use the launch
and policy tests to inspect the intended startup boundary.

## Hooks and cleanup

Every overlay gets one regular `hooks.json`, rebuilt per launch. Mine merges the
user's global JSON/TOML hook declarations with kah-owned hooks. Restricted homes
also keep user hooks, routing shell commands through gVisor; kah observer hooks
intentionally remain on the host. Hook definitions are written to `hooks.json`.
Their remapped state is stored in the `hooks.state` table in the overlay's
`config.toml`, alongside its other launch settings. State keys are remapped for the new source and group
positions, then validated against native hashes. Changed user definitions are
not silently trusted; disabled states remain disabled. Only kah-owned definitions
are newly trusted by kah. Global config and hooks files are never written.

The source tree contains Herdr fixtures for native session identity and observer
attachment. PTY fixtures use owned process groups and markers to identify their
descendants during cleanup.

Restricted native launches include an explicit `--cd` for the already-selected
project or subdirectory. Codex requires this when resuming a remote execution
workspace with `tui.resume_cwd=current`; changing only the process cwd is not
enough. The session-policy guard stays enabled: kah's hook in the native session
stops prompts and tool calls when the saved project or domain policy differs from
this launch (`SESSION_POLICY_MISMATCH`) and names the command that reopens it
correctly.

## Versioned references

The following links are versioned references for Codex `rust-v0.159.0`. See the
checked-in launch, overlay and messaging code for kah's implementation.

- [Daemon exclusion and compatibility rules](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/tui/src/daemon_startup.rs)
- [Explicit no-daemon startup](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/tui/src/startup_orchestration.rs)
- [Available native model/configuration flags](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/utils/cli/src/shared_options.rs)
- [Per-thread TUI request configuration](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/tui/src/app_server_session.rs)
- [Daemon home and endpoint selection](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/app-server-daemon/src/lib.rs)
- [Protected socket alias and hash](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/app-server-transport/src/transport/unix_socket.rs)
- [In-process TUI transport](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/app-server/src/in_process.rs)
- [Hook environment and discovery](https://github.com/openai/codex/blob/rust-v0.159.0/codex-rs/hooks/src/registry.rs)
