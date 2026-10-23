# Global setup in restricted domains

This page describes how restricted launches share installed setup and route hooks
and MCP servers.

Local stdio MCP servers use an online gVisor executor by default, while ordinary
sandbox commands follow `--online`. URL MCP connections remain native host
connections. kah does not probe servers or infer their network needs.

Optional ~/.config/kah/setup.json stores persistent operator choices:

```json
{
  "offline_mcp_servers": ["local-files"],
  "secret_mcp_servers": {
    "private-service": false,
    "approved-service": true
  }
}
```

Declare secret-bearing servers here: false disables them until the user changes
the entry to true. That is the one-time approval, reused across launches. kah
does not inspect secret values or guess whether an arbitrary script contains
secrets; unlisted servers are treated as ordinary user tools. Keep credentials
in native server configuration, not this file. Prefer fine-grained, read-only,
repository-scoped tool tokens where possible. The sandbox can read credentials
made available to enabled servers. Model subscription logins stay on the host.

Global code is mounted directly instead of frozen into skill snapshots. Only
the selected installed setup paths are shared, not the provider's parent home
or its subscription login file. Missing global instructions are left absent.
Native hook approvals carry over only when the original definition still has
its approved native hash; changed or previously unapproved definitions retain
native review behavior.

Claude uses its configured native store, which defaults to ~/.claude, for
settings, plugins, project history and its subscription credentials. kah never
copies that store and does not create per-domain credential stores. Invocation-local --settings adds permissions and
observer hooks; the native shell prefix routes existing shell hooks. Native
--mcp-config overlays route declared stdio servers without editing user config;
secret-bearing declarations use the native deniedMcpServers setting until enabled.

Research and Untrusted use native --setting-sources user: repository
.claude/settings.json and settings.local.json cannot supply host helpers,
exec-form hooks, or environment overrides. Project CLAUDE.md, skills and agents
remain available as native instructions. User-owned exec-form hooks can run on the host, while shell-form hooks use the
gVisor prefix.
