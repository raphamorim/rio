install:
	cargo install cargo-server
	cargo install wasm-bindgen-cli

run: build
	cargo server --open

build:
	cargo build -p sugarloaf-styled-text --target wasm32-unknown-unknown
	wasm-bindgen ../../target/wasm32-unknown-unknown/debug/sugarloaf_styled_text.wasm --out-dir wasm --target web --no-typescript