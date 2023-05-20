.PHONY: docs

all: install run

docs:
	cd docs && cargo server --open --port 4000

run:
	cargo run --release

dev:
	cargo run

# create-dmg macos-arm64/Rio.app 
pack-osx-arm:
	mkdir -p build
	cargo build -p rio --target aarch64-apple-darwin --release
	cd rio && MACOSX_DEPLOYMENT_TARGET=10.14 cargo bundle --release
	cp -r ./target/release/bundle/osx/* ./build/macos-arm64
	create-dmg build/macos-arm64/Rio.app

pack-osx-x86:
	mkdir -p build
	cargo build -p rio --target x86_64-apple-darwin --release
	cd rio && cargo bundle --release
	cp -r ./target/release/bundle/osx/* ./build/macos-x86
	zip -r ./build/macos-x86.zip ./build/macos-x86
	cargo build -p rio --target x86_64-apple-darwin --release
	cd rio && cargo bundle --release
	cp -r ./target/release/bundle/osx/* ./build/macos-x86
	zip -r ./build/macos-x86.zip ./build/macos-x86

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
