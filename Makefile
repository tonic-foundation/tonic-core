.DEFAULT_GOAL: help

.PHONY: help
help:
	@cat Makefile

.PHONY: dex
dex:
	cargo build --release --target wasm32-unknown-unknown -p dex

.PHONY: dex-with-debug-log
dex-with-debug-log:
	cargo build --release --target wasm32-unknown-unknown -p dex --features debug_log
