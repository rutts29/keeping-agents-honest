export CARGO_TARGET_DIR ?= $(CURDIR)/target

.DEFAULT_GOAL := check
.PHONY: check build fmt lint test size audit install-hooks install

# One recursive make per stage keeps `make -j check` in this order.
check:
	$(MAKE) size
	$(MAKE) fmt
	$(MAKE) lint
	$(MAKE) test

build:
	cargo build --locked --workspace

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --locked --workspace --all-targets -- -D warnings

test:
	bash demos/check-tests.sh cargo test --locked --workspace

size:
	cargo run --locked --quiet -p kah-check -- .

# Needs cargo-audit and an advisory database, so it is not part of check or
# the commit hook. Run it before a release.
audit:
	cargo audit

install-hooks:
	git config --local core.hooksPath .githooks

# An identical file is left alone, and a changed one lands by atomic rename,
# never as a partly written executable.
place = cmp -s "$(1)" "$(3)" || { install -Dm$(2) "$(1)" "$(3).tmp.$$$$" && mv -f "$(3).tmp.$$$$" "$(3)"; }

install:
	cargo build --locked --release -p kah-cli
	$(call place,$(CARGO_TARGET_DIR)/release/kah,755,$(HOME)/.local/bin/kah)
	$(call place,assets/shell/kah.sh,644,$(HOME)/.local/share/kah/shell.sh)
	$(call place,assets/shell/cx,755,$(HOME)/.local/bin/cx)
