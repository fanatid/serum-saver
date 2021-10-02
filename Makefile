.PHONY: build-serum-dex
build-serum-dex:
ifeq (,$(wildcard contrib/serum-dex/dex/target/deploy/serum_dex.so))
	@make build-serum-dex-force
else
	@echo "Already exists. For force build use: \`make build-serum-dex-force\`."
endif

.PHONY: build-serum-dex-force
build-serum-dex-force:
	cd contrib/serum-dex/dex && cargo build-bpf
	# Temporary, at `v0.4.0` `dex/Cargo.lock` is changed.
	cd contrib/serum-dex && git checkout dex/Cargo.lock

.PHONY: test-bpf
test-bpf: build-serum-dex
	cargo test-bpf --features="devnet"
