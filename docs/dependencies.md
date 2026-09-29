# Added dependencies

`toml = 1.1.6` parses and renders independent Codex overlay configuration. Avoids
an incomplete custom TOML parser and accidental edits to shared config. MIT/Apache
2.0, no package build script. The upstream [toml-rs repository](https://github.com/toml-rs/toml)
has history before December 2019 and was active on September 18, 2026
(`e4b8bda51c458b3c9b5671b6ad50a0a4a279f583`). Registry identity and source checked
with `cargo info toml`; transitive parser/writer/serde helpers recorded in the
lockfile. `make check` audits the resolved graph before commits.
