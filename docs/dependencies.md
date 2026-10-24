# Dependency notes

Why two less common dependencies are here, and how to audit the rest. The
checked-in manifests and `Cargo.lock` list every dependency.

- **`toml`** reads and writes kah's own Codex configuration overlay, so kah
  never hand-parses TOML or edits the user's shared Codex config by accident.
  Source and licence: [toml-rs](https://github.com/toml-rs/toml).
- **`rustix`** gives `kah-core` a safe `renameat2` with `RENAME_NOREPLACE`. A
  new run is built in a private directory and published under its run ID by one
  rename that cannot replace an existing entry. If the kernel or filesystem
  lacks that flag, creating a run fails with an error that says so, and
  nothing is published. Only Linux is supported. `rustix` is pinned to an exact
  version with the `fs` and `std` features and brings in `bitflags`, `errno`
  and `linux-raw-sys`. Licences: `rustix` and `linux-raw-sys` are
  `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`; `bitflags` and
  `errno` are `MIT OR Apache-2.0`. Source:
  [rustix](https://github.com/bytecodealliance/rustix).

## Audit

`make audit` runs `cargo audit` against `Cargo.lock`. It needs `cargo-audit`
and the RustSec advisory database, and it is separate from `make check`.
