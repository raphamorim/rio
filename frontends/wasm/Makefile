install:
	cargo install cargo-server
	cargo install wasm-bindgen-cli
	cargo install cargo-watch
	# Binaryen is also needed
	# https://github.com/WebAssembly/binaryen

run: build opt
	cargo server --open

opt:
	du -h ./wasm/rioterm_bg.wasm
	wasm-opt -O ./wasm/rioterm_bg.wasm -o ./wasm/rioterm_bg.wasm
	du -h ./wasm/rioterm_bg.wasm

watch:
	cargo watch -- make build

build:
	wasm-bindgen ../target/wasm32-unknown-unknown/debug/rioterm.wasm --out-dir wasm --target web --no-typescript

build-release:
	wasm-bindgen ../target/wasm32-unknown-unknown/release/rioterm.wasm --out-dir wasm --target web --no-typescript