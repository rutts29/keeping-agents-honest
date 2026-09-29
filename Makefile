.PHONY: check size demos install-hooks
check: size
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
	cargo audit
	$(MAKE) demos

size:
	cargo run --quiet -p kah-check -- .

demos:
	@for demo in demos/phase-*/*.sh; do bash "$$demo" || exit $$?; done

install-hooks:
	git config core.hooksPath .githooks
