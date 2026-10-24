# Your setup in Research and Untrusted

In Research and Untrusted (see [Domains](../README.md#domains)), agent commands
run in the sandbox. This page covers how your installed tools, hooks and MCP
servers are shared into it, and how to mark servers that hold secrets.

## MCP servers

- Local (stdio) MCP servers run in an online sandbox by default, while ordinary
  agent commands follow `--online`.
- URL (remote) MCP connections stay native connections from the host.
- kah does not probe servers or guess what network access they need.

## `~/.config/kah/setup.json`

This optional file stores your choices:

```json
{
  "offline_mcp_servers": ["local-files"],
  "secret_mcp_servers": {
    "private-service": false,
    "approved-service": true
  }
}
```

- `offline_mcp_servers` lists stdio servers to run in an offline sandbox.
- `secret_mcp_servers` declares servers that hold secrets. `false` disables a
  server until you change it to `true`; that one-time approval is reused
  across launches.

kah does not inspect secret values or guess whether a script contains secrets;
servers you do not list are treated as ordinary tools. Keep credentials in the
server's own configuration, not in this file, and prefer fine-grained,
read-only, repository-scoped tokens. The sandbox can read credentials that you
make available to enabled servers. Model subscription logins stay on the host.

## What is shared

- Your global setup is mounted directly instead of being frozen into
  snapshots.
- Only the selected installed setup paths are shared, not the provider's whole
  home directory or its subscription login file.
- Missing global instructions stay absent.
- Native hook approvals carry over only while a hook's definition still
  matches its approved native hash. Changed or never-approved hooks go through
  native review again.

## Claude Code

Claude Code uses its configured native store, `~/.claude` by default, for
settings, plugins, project history and its subscription credentials. kah never
copies that store and does not create per-domain credential stores.

- A `--settings` argument for that launch only adds permissions and observer
  hooks.
- The native shell prefix routes your existing shell hooks into the sandbox.
- A native `--mcp-config` argument routes declared stdio servers without
  editing your configuration. Servers declared as holding secrets stay in the
  native `deniedMcpServers` setting until you enable them.

Research and Untrusted launch Claude Code with native
`--setting-sources user`, so a repository's `.claude/settings.json` and
`settings.local.json` cannot supply host helpers, exec-form hooks or
environment overrides. Project `CLAUDE.md`, skills and agents remain available
as native instructions. Your own exec-form hooks can run on the host, while
shell-form hooks use the sandbox prefix.
