docs:
	cd docs && cargo-server --open --port 4000

run:
	cargo run --release

pack-osx:
	# cargo build --target x86_64-apple-darwin
	cargo build --target aarch64-apple-darwin
	cargo bundle

lint:
	cargo fmt -- --check --color always
	cargo clippy --all-targets --all-features -- -D warnings

watch:
	cargo watch -- cargo run

install:
	cargo install ou
	cargo install cargo-bundle
	cargo install cargo-watch
	make build

build:
	cargo build --release
