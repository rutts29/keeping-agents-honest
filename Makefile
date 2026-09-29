.PHONY: check size gate gates install-hooks
check: size
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
	cargo audit

size:
	cargo run --quiet -p kah-check -- .

gate: check
	@test -n "$(PHASE)" && test -d "demos/phase-$(PHASE)" || { echo 'Use make gate PHASE=N' >&2; exit 2; }
	@for demo in demos/phase-$(PHASE)/*.sh; do bash "$$demo" || exit $$?; done

gates: check
	@for demo in demos/phase-*/*.sh; do bash "$$demo" || exit $$?; done

install-hooks:
	git config core.hooksPath .githooks
