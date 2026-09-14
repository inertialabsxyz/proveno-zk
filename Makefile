.PHONY: help check lint test test-prove build-openvm prove-openvm prove-examples fix build

help:
	@echo "proveno-zk — proving layer (policy, commitments, Noir + OpenVM backends)"
	@echo
	@echo "  check          CI gate: lint + test"
	@echo "  lint           cargo fmt --check + cargo clippy -D warnings"
	@echo "  test           all tests"
	@echo "  test-prove     Noir nargo+bb prove/verify pipeline (slow; pre-PR gate)"
	@echo "  build-openvm   build + transpile the OpenVM guest (needs cargo-openvm)"
	@echo "  prove-openvm   full OpenVM pipeline on examples/simple.lua"
	@echo "  prove-examples run every examples/*.lua through the OpenVM pipeline"
	@echo "  fix            auto-format + apply safe clippy fixes"
	@echo
	@echo "Core runtime tests live in the proveno-core repository."

# Feature set: `zkvm` gates the commitment types, `serde` the JSON artifacts.
FEATURES = std,serde,zkvm

check: lint test

lint:
	cargo fmt --check
	cargo clippy --workspace --features "$(FEATURES)" -- -D warnings

test:
	cargo test --workspace --features "$(FEATURES)"

# Not part of `check`: ~20s and needs nargo + bb on PATH. Must pass before
# opening a PR, especially for changes to noir/, the witness writer, the oracle
# tape, canonical serialization, or the program/trace encoders. Prints
# prove/verify wall-time per test so circuit-size regressions are visible.
test-prove:
	cargo test -p proveno-noir --test prove -- --nocapture

# Needs cargo-openvm. `--no-default-features` on the guest is load-bearing:
# poseidon pulls cranelift, whose build script panics on custom RISC-V triples.
build-openvm:
	cargo openvm build -p proveno-openvm

prove-openvm:
	./prove-openvm.sh examples/simple.lua

prove-examples:
	./prove-examples.sh

fix:
	cargo fmt --all
	cargo clippy --workspace --features "$(FEATURES)" --fix --allow-dirty --allow-staged

build:
	cargo build --workspace
