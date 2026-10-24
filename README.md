# Keeping Agents Honest

Keeping Agents Honest (`kah`) is an observer-driven system that watches coding
agents (Claude Code and OpenAI Codex) and checks their work against the
user's stated intent. It records what the user asked for, reviews the agent's
committed changes against it, and steers the agent back when its work drifts.

## Start here

To understand kah in about ten minutes, open these files:

- [`crates/kah-core/src/intent_history/mod.rs`](crates/kah-core/src/intent_history/mod.rs):
  the intent ledger, a validated history of requirements, supersessions,
  conflicts and evidence.
- [`crates/kah-observer/src/verbatim.rs`](crates/kah-observer/src/verbatim.rs):
  how a user message enters the ledger; Jev chooses the typed operation and the
  code copies the exact source text.
- [`crates/kah-cli/src/observer_review/option1/admission.rs`](crates/kah-cli/src/observer_review/option1/admission.rs):
  the per-commit Jev screen and the decision to escalate to native review.
- [`crates/kah-cli/src/observer_review/launch.rs`](crates/kah-cli/src/observer_review/launch.rs):
  how the separate native observer session (Claude Code or Codex) is started for
  that review.
- [`crates/kah-cli/src/observer_review/reminder.rs`](crates/kah-cli/src/observer_review/reminder.rs):
  steering reminders delivered to the working agent through its native hooks.
- [`crates/kah-cli/src/completion_checkpoint.rs`](crates/kah-cli/src/completion_checkpoint.rs)
  and [`native_controller/completion.rs`](crates/kah-cli/src/native_controller/completion.rs):
  the checkpoint gate and the completion judgments of an unattended run.
- [`crates/kah-sandbox/src/executor.rs`](crates/kah-sandbox/src/executor.rs):
  the sandbox executor, Docker with the gVisor runtime and its container
  options.

## Domains

Every project lives under one of three directories, and the directory sets the
policy kah applies to it:

| Domain | Directory | Use it for | Agent commands | Observer review |
| --- | --- | --- | --- | --- |
| Mine | `~/repos/mine` | Your own projects | Native host permissions, network on; `--yolo` gives full host access | All three levels |
| Research | `~/repos/research` | Other people's code you are working on, such as an open-source fix | gVisor sandbox, offline unless `--online` | All three levels |
| Untrusted | `~/repos/untrusted` | Code you do not trust to run on your host | Same sandbox, offline unless `--online` | Jev screen only, as advisory flags |

Unattended runs (`kah run`) execute agent commands in the sandbox in every
domain. The review levels are described below.

## The observer

- **Intent ledger.** kah stores the user's exact words with a reference to the
  session record they came from. A message that starts with `steer:` is always
  recorded as an instruction and `aside:` marks commentary; other messages are
  classified by a typed judgment. Later instructions can supersede earlier
  requirements; each change is validated and kept in a durable intent history.
  `kah ledger` shows it and lets the user review, confirm, dismiss or promote
  items.
- **Commit review in three levels.**
  1. Jev typed judgments check each observed change to Git HEAD against the
     recorded intent.
  2. In Mine and Research, a separate native observer session reviews the
     change when that is needed and reports with cited source evidence.
  3. In Mine and Research, checkpoint reviews judge the delivered range as a
     whole.

  Untrusted uses the first level only and returns advisory flags. An
  unattended run completes only when its tests pass and both completion
  judgments confirm the required outcomes, plus its checkpoint review where
  native review is enabled.
- **Steering.** Findings reach the working agent as reminders through the
  provider's native hooks, quoting the user's own words. Users can also send
  messages to a session (`kah steer`, `kah send`) or relay an exact quote from
  one session to another (`kah relay`).
- **Evidence.** Native review reports cite source evidence, which kah validates
  before it applies a decision. Judgments, review records and run reports are
  kept in the kah state root; `kah status` shows session, review and run state.
- **Unattended runs.** `kah run` starts an agent in its own Git worktree. kah
  imports the agent's commit into a separate candidate and tests it in a fresh
  checkout. A run completes only after the test command passes and the
  observer's completion checks pass. The result is a local `kah/run-ID` branch
  and a report. The branch appears as soon as the commit is captured, before
  verification finishes, so check the run report and status before merging. kah
  does not merge, push or deploy.

**Results.** On real work, safetensors issue #878, kah's LLM review caught three
defects in the code Codex was committing, and the agent fixed each in its next
commit about 170 to 230 seconds after the finding. In a planted-violation
replay, the first-level Jev screen escalated seven of eight violations and none
of 23 controls, at about $0.0024 per commit for the screen. See the
[evidence summary](docs/evidence-summary.md).

The native Claude Code and Codex clients keep running their own sessions and
models. kah does not replace them.

```sh
kah research NAME                  # Codex in a sandboxed Research project
kah --agent claude mine NAME       # Claude Code in a Mine project
kah --no-observer research NAME    # the observer is on by default; this opts out
kah run --test COMMAND TASK        # unattended run, inside Herdr
kah steer RUN TEXT; kah status RUN; kah logs RUN; kah stop RUN; kah resume RUN
```

`kah --help` prints usage and common options;
[docs/COMMANDS.md](docs/COMMANDS.md) lists the rest.

## How it works

```
 user ──> native agent (Claude Code or Codex) ──> commits
             ^                │ session records, hooks
             │ reminders      v
             │       kah: intent history (verbatim user text)
             │                │
             │                v
             └──── commit checks (Jev); native review when needed in Mine/Research
```

| Crate | Role |
| --- | --- |
| `kah-cli` | The `kah` binary: command routing, native launch, host hooks, review orchestration, unattended run control |
| `kah-core` | Provider-neutral intent history, ledger, projections, session bundles, run state, verification data |
| `kah-observer` | Intent intake, chronology, review evidence, requirement decisions, context preparation, Jev client |
| `kah-codex` | Codex session intake, control and delivery, hooks, overlays, usage parsing |
| `kah-claude` | Claude Code transcript, lifecycle, paste and usage parsing; native configuration paths |
| `kah-sandbox` | Directory policy, project setup, sandbox executor, setup mounts, Git administration protection |
| `kah-run` | Worktrees, candidate transfer, verification, process lifecycle, leases, persisted run controls |
| `kah-check` | Source file size checks used by the development check; a Codex skill-discovery probe (example) and its skill-source check |

**Typed judgments.** kah asks the TypeSafe Jev API (https://docs.typesafe.ai)
structured questions about intent and evidence and gets back typed answers
with probability and confidence values. Decisions that depend on those values
use configured thresholds. Jev is the only implemented judgment backend.

## The sandbox

In Research and Untrusted (see [Domains](#domains)), and in every unattended
run, agent commands run in the sandbox:

- **gVisor under a separate account.** Commands run in containers of the
  sandbox service account's rootless Docker daemon with the gVisor runtime
  (`runsc`). On the author's server gVisor runs on a host without KVM.
- **Locked-down containers.** Read-only root filesystem, `--cap-drop ALL`,
  `no-new-privileges`, and network `none` unless `--online` is given. The
  project directory is writable; the native Codex binary is mounted read-only
  as the command server inside the container.
- **Logins stay on the host.** The native clients, their logins and kah's hooks
  run on the host. For Claude Code, Bash runs in gVisor and the native file
  tools stay on the host under kah's permission rules. Provider traffic, hosted
  search and configured MCP servers use separate network paths.
- **Fail closed.** Missing image-tag or service-account setup prevents launch.
  If a sandbox cannot start, that command fails; kah does not fall back to
  executing it on the host.

Code: container options in
[`crates/kah-sandbox/src/executor.rs`](crates/kah-sandbox/src/executor.rs),
directory policy in [`crates/kah-sandbox/src/policy.rs`](crates/kah-sandbox/src/policy.rs)
and [`project.rs`](crates/kah-sandbox/src/project.rs), sandbox setup per launch
in [`crates/kah-cli/src/native_setup.rs`](crates/kah-cli/src/native_setup.rs),
Claude Code's file-tool rules in
[`crates/kah-cli/src/claude_policy.rs`](crates/kah-cli/src/claude_policy.rs).

## Scope

This repository is proof of work: the source of kah and of the sandboxed
deployment it runs in on the author's own server. That deployment is kah with
Claude Code and Codex, a gVisor sandbox under a dedicated service account, and
Herdr panes. It is why the requirements below list that full custom setup.
The ideas are free to borrow.

- `make install` builds and installs `kah` and its shell helpers. Docker, gVisor,
  the service account, the sandbox image, the agent CLIs, Herdr and logins are
  part of the host setup listed under [Requirements](#requirements).
- Only the final commit is expected to build. Older commits in the history are
  kept for the record.

### Run it on your own server

These are starting points for adapting it to another host; a choice can be used
in more than one place, so search for each before changing it:

| What | Where |
| --- | --- |
| Sandbox image and its tag | `assets/sandbox/Dockerfile`; the tag kah runs is read from `/usr/local/share/sbx/image-ref` |
| Service account name | `crates/kah-run/src/access.rs` |
| Docker socket, runtime and container options | `crates/kah-sandbox/src/executor.rs` |
| Setup shared into the sandbox | `crates/kah-sandbox/src/setup.rs` |
| Project roots (`~/repos`) and domains | Parsing and rules in `crates/kah-sandbox/src/project.rs`, `policy.rs`; the `~/repos` choice in `crates/kah-cli/src/launch.rs`, `claude_launch.rs`, `resume.rs`, `unattended_setup.rs`; domain checks in `crates/kah-cli/src/observer_review/control.rs`, `observer_review/untrusted/host.rs` |
| Native CLI paths (`~/.local/bin/codex`, `~/.local/bin/claude`) | `crates/kah-cli/src/native_setup.rs` (sandbox copy of Codex), `launch.rs`, `unattended_setup.rs`, `claude_launch.rs` |
| Host tool paths (`/usr/bin/...`) | listed under [Requirements](#requirements) |

### Roadmap

Next: kah as a standalone install. The observer (intent ledger, commit review,
steering) separated from the sandbox, so that it can be set up in other agent
harnesses with a short setup.

## Requirements

To build:

- Linux. The code uses Unix sockets, file locks, process groups and `/proc`.
- Rust 1.97.1 with rustfmt and Clippy, as pinned in `rust-toolchain.toml`.
  The crates use edition 2024 and Cargo resolver 3; no minimum supported Rust
  version is declared.
- A C compiler and linker (the dependency graph includes `ring`).
- Access to crates.io or a populated Cargo cache. Dependencies are not
  vendored.
- `cargo-audit` and access to the RustSec advisory database for `make audit`.

To run the default tests, also:

- Git, GNU Make, Bash, GNU coreutils and findutils, and grep.
- ACL tools (`getfacl`, `setfacl`) and a temporary filesystem with POSIX ACL
  support. Eight default tests grant an ACL entry to another user ID, which
  fails in a user namespace that maps only the caller's ID (for example a
  rootless container or bubblewrap); run `make test` on the host.
- Node.js and jq.
- util-linux and procps tools such as `setpriv`, `script`, `kill` and
  `pidwait`, and PTY support.
- A writable temporary directory that allows executables.

To run kah itself, as on the author's server:

- Codex at `~/.local/bin/codex` with its native login. kah needs it even when
  the agent is Claude Code: every unattended run, every native observer
  review and every Research or Untrusted session uses it, and the sandbox
  runs it as the command server inside each container. To read native Codex
  session records, kah also starts `codex app-server` found on `PATH`, or the
  program named by `KAH_CODEX_BIN` when that is set.
- Claude Code at `~/.local/bin/claude` with its native login, to use Claude
  Code as the agent.
- Herdr (https://herdr.dev) is the terminal pane server kah uses; unattended
  runs and pane messaging need it. It is a separate program and is not part of
  this repository. Unattended runs start inside a live Herdr session. kah sends
  supervision alerts to the Herdr agent named by `KAH_REVIEWER_AGENT`
  (default `kah-reviewer`). A name must start with a lowercase letter and use
  at most 32 lowercase letters, digits, `-` or `_`; kah warns and uses the
  default for any other value.
- The sandbox service account, under the fixed account name that
  `crates/kah-run/src/access.rs` looks up, its rootless Docker daemon with the
  gVisor runtime, and a sandbox image (`assets/sandbox/Dockerfile` is the
  image recipe; it targets x86_64). kah reads the image tag to run from
  `/usr/local/share/sbx/image-ref`, which `make install` does not create;
  `assets/sandbox/image-ref` holds the tag the recipe was built and tested as.
- These host programs at fixed paths: `/usr/bin/docker`, `/usr/bin/git`,
  `/usr/bin/setpriv`, `/usr/bin/setfacl`, `/usr/bin/getfacl`,
  `/usr/bin/timeout`, `/usr/bin/pidwait`, `/usr/bin/find` and `/usr/bin/id`.
- A TypeSafe API key for Jev (see [Data flow](#data-flow)).

## Build and test

```sh
# Build the workspace
cargo build --locked --workspace

# Build the kah binary (target/release/kah)
cargo build --locked --release -p kah-cli

# Quick tests that need no external tools
cargo test --locked -p kah-check -p kah-claude

# Default workspace tests. The wrapper gives the run its own state and
# temporary directories, under ${TMPDIR:-/tmp}/kah-check-tests or
# KAH_TEST_FIXTURE_BASE
make test

# Full local check: size, formatting, Clippy and the default tests
make check

# Dependency audit (needs cargo-audit); not part of make check; see docs/dependencies.md for known advisories
make audit

# Optional: run make check before each commit
make install-hooks
```

Run the workspace tests through `make test` (or `demos/check-tests.sh`).
Several tests require the isolated state directory that the wrapper sets, so
a plain `cargo test --workspace` fails.

Host-dependent test cases are marked `#[ignore]` and do not run by default.
These include tests that use the sandbox service account, Docker and gVisor,
the sandbox image, Herdr, a real Codex or Claude Code login, or the TypeSafe
API. The test wrapper also clears the environment variables that turn some
default test entrypoints into live runs. Live tests can use paid provider
usage. Run an ignored test only by name, after reading its prerequisites.
`cargo test -- --ignored` is not a supported suite: it mixes live provider
calls, sandbox checks and local probes. [AGENTS.md](AGENTS.md) has the
contributor rules.

## Data flow

When kah uses the Jev judge, it sends selected user text and code evidence
(such as diffs and review excerpts) to the TypeSafe API at
`https://api.typesafe.ai/v1/systemone`. Some review and completion paths
scrub common credential patterns before sending. This does not cover every
Jev request: intent intake can send original user text. Scrubbing is not a
confidentiality guarantee. When kah uses a native Claude Code or Codex
observer session, the intent and diffs it reviews also go to that provider.

The Jev API key is read from `~/.config/kah/typesafe.key`. It must be a
regular file (not a symlink) owned by the home directory's owner, with mode
`0600`. Mock and loopback replay clients used in tests need no key.

The ledger, run state, Codex overlays and Jev service record use
`$XDG_STATE_HOME/kah` when `XDG_STATE_HOME` is absolute, otherwise
`$HOME/.local/state/kah`; a Codex overlay made in the default location before
`XDG_STATE_HOME` was set stays in use for its sessions. In Research and
Untrusted, Claude Code's file-tool rules deny that directory. kah also keeps
observer workspaces under
`$HOME/.cache/kah-observers` and evaluation/delivery records under
`$HOME/.local/share/kah-eval`. Native clients keep their own session records.

## Documentation

[docs/README.md](docs/README.md) indexes the component guides.
[docs/evidence-summary.md](docs/evidence-summary.md) states how kah was
verified during development.

## Licence

MIT. See [LICENSE](LICENSE). kah includes code adapted from driftctl (MIT);
see [NOTICE](NOTICE).
