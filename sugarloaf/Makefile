install:
	cargo install cargo-server
	cargo install wasm-bindgen-cli

dev:
	cargo run --example text

run: build
	cargo server

## Firefox's geckodriver
## cargo install geckodriver
test-firefox:
	GECKODRIVER=geckodriver cargo test -p sugarloaf --tests --target wasm32-unknown-unknown

test:
	GECKODRIVER=chromedriver cargo test -p sugarloaf --tests --target wasm32-unknown-unknown

build:
	cargo build -p sugarloaf-wasm --target wasm32-unknown-unknown
	wasm-bindgen ../target/wasm32-unknown-unknown/debug/sugarloaf_wasm.wasm --out-dir wasm --target web --no-typescript
