# Unattended work

From an existing project under `~/repos/{mine,research,untrusted}`:

```sh
kah run --model gpt-6-sol --effort low --time-limit 600 --test 'make check' 'Implement the requested change'
kah steer run-ID 'Use the revised behavior instead'
kah status run-ID
kah logs run-ID
kah stop run-ID
kah resume run-ID
```

Model and effort are optional; omitted settings follow native Codex configuration.
The observer's text model/effort and Jev alias are separately configurable with
`--observer-model`, `--observer-effort`, and `--jev-model`.

A run starts from committed HEAD in a private Git-backed worktree. Uncommitted
source edits are not imported. Native commands and edits run in gVisor for every
domain, including Mine. Git metadata is read-only to the worker. The controller
makes local milestone commits and exports only `kah/run-ID` into the source
repository. It never checks out, merges, pushes or deploys that branch.

Each verification starts in a fresh detached checkout of the candidate commit;
ignored worker files cannot satisfy it. Tests run in gVisor. The test command
should be the project's normal verification entrypoint, so steering can update
the project tests without changing the host command. Temporary binaries must
run in the verification checkout, since `/tmp` is non-executable.

Completion requires passing verification without candidate changes, fresh user
sequence/checkpoint matching, and two independent Jev judgments satisfying every
recorded outcome. Per-outcome Choices distinguish satisfied, violated and unclear.
The required probability is 0.90 and confidence is 0.85. Worker assertions are not completion. Failed or uncertain
runs stop with a report; a steer and `kah resume` can continue them.

The observer records the task and explicit `kah steer` messages as user authority.
Native output and judgment text remain evidence. Jev is the judge in all domains;
source-linked text extraction uses the separately configured native model.
Unattended native subagent and goal tools are disabled: one worker's lifecycle
and allowance are controlled by kah. Interactive native behavior is unchanged.

`--budget-tokens N` counts reported worker and observer tokens, including cached
input tokens. Usage arrives at native turn completion, so the allowance can
overshoot by one bounded call. Interrupted calls may have unavailable usage;
reported counts must not be mistaken for a billing total. `--time-limit SECONDS`
is cumulative across resume; bounded in-flight observation/verification can delay
wind-down. Resume preserves counters and refuses an exhausted allowance. kah
never resets provider usage or requests a reset.

A killed coordinator's native worker receives a parent-death signal. Resume
acquires the run lease, removes remaining labeled containers, and restores the
last checkpoint before continuing with the saved ledger. Uncommitted private
worktree edits are discarded by this explicit resume operation.

Research can opt into read-only HTTPS fetching:

```sh
kah run --domain research --allow-host example.com --test 'make check' 'Use the approved reference'
```

The worker receives `/opt/kah __fetch https://example.com/path`. Raw network access
stays disabled. The helper permits GET only, exact hosts on port 443, public
resolved addresses, no redirects/proxies/credentials, and bounded replies.
A per-run file exchange avoids changing gVisor's disabled host-socket policy.
This is a deliberate read-only fetch capability, not general online networking.
Mine and Untrusted reject allowlists. No host credentials or Docker socket enter
the sandbox; installed user skills are available as read-only snapshots.

On stop, block or completion, kah emits a local terminal notification and a JSON
report under `~/.local/state/kah/runs/run-ID/report.json`: goal, result, branch,
diff statistics, commit-bound tests, open question, ledger summary and usage.
Review the local branch diff, then merge it yourself. Native workers use the
supported `codex exec --ephemeral` flag and do not enter everyday resume history.
