# Security policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub: on the repository's
**Security** tab, choose **Report a vulnerability**
(https://github.com/rutts29/keeping-agents-honest/security/advisories/new).
Do not open a public issue for a security problem.

Include the kah commit, the agent client and its version (Claude Code or
Codex), the domain (Mine, Research or Untrusted), and the steps to reproduce.
I will acknowledge the report and keep you updated while it is fixed.

## Supported versions

Only the latest commit on `main` is supported.

## Scope

In scope:

- Escaping the gVisor sandbox, or reaching host files outside the project it
  mounts.
- A repository's own files, such as agent settings, Git hooks or Git config,
  running code on the host.
- Exposure of the developer's logins, the TypeSafe key or other credentials.
- Text that is not the developer's own words becoming a rule in the intent
  ledger.

Out of scope:

- Vulnerabilities in Claude Code, Codex, Docker, gVisor or Herdr themselves.
  Report those to their own projects.
- Attacks that need the developer's host account to be compromised already.
