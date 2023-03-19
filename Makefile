.PHONY: docs

all: install run

docs:
	cd docs && cargo server --open --port 4000

run:
	cargo run --release

dev:
	cargo run

pack-osx:
	# cargo build --target x86_64-apple-darwin
	cargo build --target aarch64-apple-darwin
	cargo bundle

lint:
	cargo fmt -- --check --color always
	cargo clippy --all-targets --all-features -- -D warnings

test:
	make lint
	RUST_BACKTRACE=full cargo test --release

watch:
	cargo watch -- cargo run

install:
	cargo install cargo-server
	cargo install cargo-bundle
	cargo install cargo-watch
	cargo build --release

build:
	cargo build --release
