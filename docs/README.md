# kah documentation

These guides explain how to use kah and how its parts work. Start with the
[README](../README.md) for what kah is, the three [domains](../README.md#domains)
(Mine, Research, Untrusted) and the requirements.

## Using kah

- [Interactive launch](interactive.md): starting a session, launch options and
  what each domain allows.
- [Daily commands](COMMANDS.md): everyday commands and the full command
  reference.
- [User controls](user-controls.md): `steer:` and `aside:`, reviewing the
  ledger, and sending messages to a session.
- [Unattended work](unattended.md): `kah run`, its controls, verification and
  resume.
- [Relays and dispatch](relay.md): passing a user's exact words from one
  session to another.
- [Setup policy](setup.md): how your installed setup, hooks and MCP servers
  are shared with sandboxed sessions.

## How it works

- [Observer judgments](judge-backends.md): how Jev answers are used, and the
  decision thresholds.
- [Claude execution](claude-adapter.md): how kah runs Claude Code and delivers
  reminders.
- [Startup and transport](startup-modes.md): how Codex sessions start and how
  messages reach them.
- [Overlay recovery](codex-overlay-recovery.md): repairing shared Codex files.
- [Component maps](component-map.md): mapping changed files to requirements
  and reusing review evidence.
- [Dependency notes](dependencies.md): notable dependencies and `make audit`.
- [Evidence summary](evidence-summary.md): what kah did on real and controlled
  work during development.
