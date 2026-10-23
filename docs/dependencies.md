# Dependency notes

`toml` parses and renders the independent Codex overlay configuration. It avoids
an incomplete custom TOML parser and accidental edits to shared config. The
Cargo manifests pin the dependency, and `Cargo.lock` records its resolved
dependency graph, including the transitive parser, writer and serde helpers. The
upstream [toml-rs repository](https://github.com/toml-rs/toml) publishes its
source and licensing information.

`rustix` gives `kah-core` a safe wrapper for `renameat2` with `RENAME_NOREPLACE`.
A new run is built in a private sibling directory and published under its run ID
by one rename that cannot replace an existing entry, which a plain `rename` or a
check followed by a rename cannot guarantee. There is no fallback: if the kernel
or filesystem lacks the flag, creating a run fails with an error that says so,
and nothing is published. Only Linux is implemented. The `kah-core` manifest pins
`rustix` to an exact version with the `fs` and `std` features. `Cargo.lock` adds `rustix` and its three transitive
crates, `bitflags`, `errno` and `linux-raw-sys`. `rustix` and `linux-raw-sys` are
licensed `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`; `bitflags` and
`errno` are `MIT OR Apache-2.0`. The upstream
[rustix repository](https://github.com/bytecodealliance/rustix) publishes its
source and licensing information.

Use the checked-in manifests and lockfile to identify the dependencies in this
tree. `make audit` runs `cargo audit` against that lockfile; it needs
`cargo-audit` and an advisory database, and it is separate from `make check`.
At the time of this release `make audit` reports two known advisories,
RUSTSEC-2026-0381 and RUSTSEC-2026-0382; they are tracked and not yet resolved.
