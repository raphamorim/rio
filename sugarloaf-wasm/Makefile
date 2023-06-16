install:
	cargo install cargo-server
	cargo install wasm-bindgen-cli
	npm install

run: build
	cargo server

build:
	cargo build -p sugarloaf-wasm --target wasm32-unknown-unknown
	wasm-bindgen ../target/wasm32-unknown-unknown/debug/sugarloaf_wasm.wasm --out-dir wasm --target web --no-typescript

# test:
# 	wasm-pack test --headless --firefox
