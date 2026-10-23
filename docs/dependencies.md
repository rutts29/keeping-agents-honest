# Dependency notes

`toml` parses and renders the independent Codex overlay configuration. It avoids
an incomplete custom TOML parser and accidental edits to shared config. The
Cargo manifests pin the dependency, and `Cargo.lock` records its resolved
dependency graph, including the transitive parser, writer and serde helpers. The
upstream [toml-rs repository](https://github.com/toml-rs/toml) publishes its
source and licensing information.

Use the checked-in manifests and lockfile to identify the dependencies in this
tree. `make audit` runs `cargo audit` against that lockfile; it needs
`cargo-audit` and an advisory database, and it is separate from `make check`.
At the time of this release `make audit` reports two known advisories,
RUSTSEC-2026-0381 and RUSTSEC-2026-0382; they are tracked and not yet resolved.
