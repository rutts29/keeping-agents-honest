# Claude Code under kah

Where Claude Code runs under kah, which of its tools are sandboxed, and how
kah's reminders reach it. For launch commands see
[interactive sessions](interactive.md); for shared tools and MCP servers see
[setup policy](setup.md).

## Where things run

- The Claude Code process, its transcript writer and kah's observer hooks run
  on the host, outside the command sandbox.
- kah uses your native Claude login. It does not add a credential gateway or
  configure the provider's enterprise gateway.
- kah runs the native Claude executable at `~/.local/bin/claude`; no setting
  selects another path. It does not bundle or update the client. Client
  updates can change hook and permission behavior, so include the client
  version when you report a problem.

## Domains

Claude Code works in all three domains (see
[Domains](../README.md#domains)). Settings apply to that launch only; your
global provider settings are not edited.

- **Mine** uses host workspace permissions.
- **Research and Untrusted** keep Claude's native project file tools and
  protect private host paths with permission rules. Bash runs in the gVisor
  sandbox through `CLAUDE_CODE_SHELL_PREFIX`. The file tools stay on the host:
  their boundary is the permission rules, not gVisor.

## Unattended runs

Unattended runs use a full native interactive session. kah rejects Claude
print mode (`-p`, `--print`) and background mode. The client stays on the
host and the agent's commands run in gVisor.

## Credentials

Model login files stay outside the sandbox mounts. Credentials that you
deliberately give to an enabled tool, such as an MCP server, can still reach
that tool; see [setup policy](setup.md).

## Whose words count as user intent

Native subagents remain available. kah takes user intent only from the primary
session's user records; provider metadata and subagent (sidechain) content do
not count as user words.

Relayed instructions must match their saved host receipts before kah imports
them. If the required source record is missing from the transcript, kah
records a gap.

## How reminders reach Claude

kah sends worker context through the hook field `additionalContext` and a
notice for you through `systemMessage`. A reminder contains the observer's
instructions, quotes of your recorded intent and references to the full
intent.

Each hook message is limited to 10,000 UTF-16 code units, counting the part
marker. kah does not rely on Claude reading a provider overflow file.

- A short reminder goes out as one hook message.
- A longer reminder is split into ordered parts, each starting with a marker
  such as `[kah message part 2/3; continues at your next step]`. A part ends
  at a line end when it can.
- Later parts go out on the agent's next eligible hooks. A partly delivered
  reminder keeps its place in the queue, so no other reminder goes first, and
  no text is dropped.
- Codex reminders are delivered whole.
- An observer message typed into a pane must still pass the client's native
  size check.

## Compatibility

Hook and CLI behavior can change between Claude Code versions. Check the
installed client's `--help` and the official
[hooks reference](https://code.claude.com/docs/en/hooks) for the events and
payload fields that version supports. Other references:

- [Sandboxing](https://code.claude.com/docs/en/sandboxing): Claude's own Bash
  sandbox is separate from kah's gVisor shell prefix.
- [Environment variables](https://code.claude.com/docs/en/env-vars):
  `CLAUDE_CODE_SHELL_PREFIX` wraps shell commands.
- [CLI reference](https://code.claude.com/docs/en/cli-reference): model,
  effort and resume options.
