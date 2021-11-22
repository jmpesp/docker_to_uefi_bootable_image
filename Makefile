
.PHONY: all clippy fmt test build clean fix

all: clippy

clippy: fmt
	cargo clippy

fmt: test
	cargo fmt

test: build
	cargo test

build:
	cargo build

clean:
	cargo clean

fix:
	cargo fix --allow-dirty

