# Project working agreement

The user chooses the outcome. Inspect the project before choosing a language,
framework or task plan. Keep user requirements distinct from your assumptions;
preserve local conventions and run checks relevant to each change.

## Git continuity

- This project has a committed Git baseline. Work in this repository; do not
  replace its Git history or silently move work to another project.
- Make a descriptive local commit after each coherent, checked milestone and
  before a long pause, risky refactor or handoff. Choose milestones by actual
  work, not a timer. Report the latest commit and any remaining dirty files.
- Review staged paths before committing. Never commit credentials, private
  keys, tokens, personal session data, dependency caches or generated output.
  A local commit is not an off-host backup; ask before creating or pushing a
  remote unless the user has already authorized it.
- In the Research sandbox, Git sees the selected bind at /workspace under a
  different host UID. Use the command-scoped form
  `git -c safe.directory=/workspace -C /workspace ...` for that exact
  repo; never set a wildcard or host-global safe-directory exception.

## Dependency decisions

- Prefer the standard library or an existing dependency when it meets the
  need. Choose a new package for a concrete benefit, not because it is popular.
- Before adding one, verify the exact registry name and source, maintainer and
  release history, recent ownership or version changes, license, install
  scripts, transitive additions and lockfile diff. A first release, sudden
  maintainer change, lookalike name or unexplained install hook calls for more
  scrutiny. Obey any configured package-manager release-age floor; age and
  download count alone still do not establish trust.
- Check known advisories for the selected version using the ecosystem's
  available audit tool or an authoritative advisory source. Examples: npm
  audit for a Node lockfile, RustSec/cargo-audit for Cargo.lock, PyPA
  pip-audit for Python, or OSV/GitHub advisories. An empty advisory result is
  not proof that a package is trustworthy. Record unresolved findings and
  choose a lower-risk alternative when provenance is weak.
- Preserve and commit supported lockfiles. Do not silently run a broad
  upgrade, install an audit tool, suppress an advisory or execute an
  unnecessary package lifecycle script merely to finish the task.
- Respect the selected CX route: Research and Untrusted are offline unless
  the user explicitly starts an online session. If an advisory lookup is
  unavailable offline, use local evidence and state the uncertainty. Do not bypass the route to perform the lookup.

Authoritative references when network use is permitted:
[GitHub dependency review](https://docs.github.com/en/code-security/concepts/supply-chain-security/dependency-review),
[OSV](https://osv.dev/),
[npm audit](https://docs.npmjs.com/cli/audit.html/),
[RustSec cargo-audit](https://github.com/rustsec/rustsec/blob/main/cargo-audit/README.md),
[PyPA pip-audit](https://github.com/pypa/pip-audit).
