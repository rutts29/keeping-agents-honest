# Claude execution and hook delivery

This page describes how kah runs Claude Code and delivers hook output. For the
launch commands, see [interactive launch](interactive.md); for the shared setup
policy, see [setup policy](setup.md).

## Execution model

The native Claude process stays on the host. kah uses the native Claude login and
does not implement a model credential gateway or configure the provider's
enterprise gateway. Claude's transcript writer and kah's observer hooks also run
on the host, outside the command sandbox.

kah invokes the native Claude executable at `~/.local/bin/claude` through its
launcher; no setting selects another path. It
does not bundle a provider binary and does not invoke an updater. Native-client
updates can change hook and permission behavior, so include the native client
version when reporting an adapter issue.

All three domains (Mine, Research and Untrusted) launch Claude. Settings are
invocation-local: the user's global provider settings are not edited.

- **Mine** uses host workspace permissions.
- **Research and Untrusted** keep Claude's native project file tools and protect
  private host paths with permission rules. Bash runs in gVisor through
  `CLAUDE_CODE_SHELL_PREFIX`. Bash isolation alone does not place Claude's
  native file tools inside gVisor; their boundary is the permission rules.

Unattended work uses a full native interactive session. kah rejects Claude print
mode (`-p`, `--print`) and background mode. Unattended Claude keeps its
controller on the host and runs worker commands in gVisor.

Model login files stay outside worker mounts. Credentials that the user
deliberately supplies to an enabled tool (for example an MCP server) can still
reach that tool; see [setup policy](setup.md).

## Provenance

Native subagents remain available. kah's user-authority intake is based on the
primary session: it preserves authoritative user transcript records and
excludes provider metadata and sidechain content from user authority. Claude
tools and Codex executor tools have different execution boundaries.

Relayed instructions must match their saved host receipts before import. kah
reads source records from the native transcript and records a gap when the
required evidence is unavailable.

## Reminder delivery

kah uses hook `additionalContext` to send worker context and `systemMessage` for
a notice that is shown to the user. Queued reminders contain observer-composed
instructions, validated intent quotes and references to the complete published
intent.

kah's reminder splitter uses a limit of 10,000 UTF-16 code units per hook
message, counted before each part is formed and including the part marker. kah
does not rely on the worker reading a provider-generated overflow file.

- A short reminder is emitted as one hook message.
- A longer reminder is divided into ordered parts, each starting with a marker
  such as `[kah message part 2/3; continues at your next step]`. A part ends at a
  line end when it can.
- Further parts are emitted on later eligible worker hooks. The remaining text
  stays queued, and a partly delivered reminder keeps its queue position, so no
  other reminder goes first. No text is discarded.
- Codex reminders are delivered whole.
- An observer message typed into a pane must still pass the native-size check.

The splitter is in `crates/kah-cli/src/observer_review/reminder.rs`. Other hook
payloads remain subject to the native client's behavior.

## Compatibility

Provider CLI and hook behavior can differ by version. When diagnosing a
compatibility problem, check the installed client's `--help` and the official
[hooks reference](https://code.claude.com/docs/en/hooks) for the events and
payload fields that version supports. Other provider references:

- [Sandboxing](https://code.claude.com/docs/en/sandboxing): the native Bash
  sandbox is separate from kah's gVisor shell prefix.
- [Environment variables](https://code.claude.com/docs/en/env-vars):
  `CLAUDE_CODE_SHELL_PREFIX` wraps shell commands.
- [CLI reference](https://code.claude.com/docs/en/cli-reference): model, effort
  and resume options accepted by the installed client.

The checked-in launch, setup, hook and transcript modules and their tests define
the configured behavior.
