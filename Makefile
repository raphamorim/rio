docs:
	cd docs && ou --open --port 4000

osx:
	cargo build --target x86_64-apple-darwin
	cargo bundle

watch:
	cargo watch -- cargo run

setup:
	cargo install ou
	cargo install cargo-bundle
