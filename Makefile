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

# Parallel gate runs install at once: an identical file is left alone, and a
# changed one lands by atomic rename, never as a partly written executable.
place = cmp -s "$(1)" "$(3)" || { install -Dm$(2) "$(1)" "$(3).tmp.$$$$" && mv -f "$(3).tmp.$$$$" "$(3)"; }

install:
	cargo build --release -p kah-cli
	$(call place,$(CARGO_TARGET_DIR)/release/kah,755,$(HOME)/.local/bin/kah)
	$(call place,assets/shell/kah.sh,644,$(HOME)/.local/share/kah/shell.sh)
	$(call place,assets/shell/cx,755,$(HOME)/.local/bin/cx)
