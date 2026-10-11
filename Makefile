export CARGO_TARGET_DIR ?= $(CURDIR)/target

.PHONY: check size gate gates disk-check install-hooks install
check: size
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	bash demos/check-tests.sh
	cargo audit

size:
	cargo run --quiet -p kah-check -- .

disk-check:
	@KAH_GATE_PHASE="$(PHASE)" bash demos/check-disk.sh

gate: disk-check
	@test -n "$(PHASE)" && test -d "demos/phase-$(PHASE)" || { echo 'Use make gate PHASE=N' >&2; exit 2; }
	$(MAKE) check
	@bash demos/run-gate.sh "$(PHASE)"

gates: disk-check
	$(MAKE) check
	@bash demos/run-gate.sh all

install-hooks:
	git config core.hooksPath .githooks

install:
	cargo build --release -p kah-cli
	install -Dm755 "$(CARGO_TARGET_DIR)/release/kah" "$(HOME)/.local/bin/kah"
	install -Dm644 assets/shell/kah.sh "$(HOME)/.local/share/kah/shell.sh"
	install -Dm755 assets/shell/cx "$(HOME)/.local/bin/cx"
