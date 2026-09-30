.PHONY: check size gate gates install-hooks install
check: size
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
	cargo audit

size:
	cargo run --quiet -p kah-check -- .

gate: check
	@test -n "$(PHASE)" && test -d "demos/phase-$(PHASE)" || { echo 'Use make gate PHASE=N' >&2; exit 2; }
	@bash demos/run-gate.sh "$(PHASE)"

gates: check
	@bash demos/run-gate.sh all

install-hooks:
	git config core.hooksPath .githooks

install:
	cargo build --release -p kah-cli
	install -Dm755 target/release/kah "$(HOME)/.local/bin/kah"
	install -Dm644 assets/shell/kah.sh "$(HOME)/.local/share/kah/shell.sh"
	install -Dm755 assets/shell/cx "$(HOME)/.local/bin/cx"
