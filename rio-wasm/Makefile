install:
	cargo install cargo-server
	cargo install wasm-bindgen-cli
	cargo install cargo-watch

run: build
	cargo server --open

opt:
	# https://github.com/WebAssembly/binaryen
	du -h ./wasm/rio_wasm.wasm
	wasm-opt -O ./wasm/rio_wasm.wasm -o ./wasm/rio_wasm.wasm
	du -h ./wasm/rio_wasm.wasm

watch:
	cargo watch -- make build

build:
	cargo build -p rio-wasm --target wasm32-unknown-unknown
	wasm-bindgen ../target/wasm32-unknown-unknown/debug/rio_wasm.wasm --out-dir wasm --target web --no-typescript

build-release:
	cargo build -p rio-wasm --target wasm32-unknown-unknown --release
	wasm-bindgen ../target/wasm32-unknown-unknown/release/rio_wasm.wasm --out-dir wasm --target web --no-typescript