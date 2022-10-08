docs:
	cd docs && cargo-server --open --port 4000

osx:
	cargo build --target x86_64-apple-darwin
	cargo bundle

lint:
	cargo fmt -- --check --color always
	cargo clippy --all-targets --all-features -- -D warnings

watch:
	cargo watch -- cargo run

setup:
	cargo install ou
	cargo install cargo-bundle
