# Sugarloaf

Sugarloaf is Rio rendering engine, desgined to be multiplatform. It is based on WebGPU, Rust library for Desktops and WebAssembly for Web (JavaScript). This project is created and maintained for Rio terminal purposes but feel free to use it.

### Desktop rect

```bash
cargo run --example rect
```

### Desktop text

```bash
cargo run --example text
```

## Examples

| ![Demo sugarloaf 1](https://github.com/raphamorim/rio/blob/main/sugarloaf/resources/demo-sugarloaf-1.png?raw=true) | ![Demo Sugarloaf wasm](https://github.com/raphamorim/rio/blob/main/sugarloaf/resources/demo-wasm-1.png?raw=true) |
| ----------- | ----------- |
| ![Demo Rect](https://github.com/raphamorim/rio/blob/main/sugarloaf/resources/demo-rect.png?raw=true) | ![Demo sugarloaf 3](https://github.com/raphamorim/rio/blob/main/sugarloaf/resources/demo-sugarloaf-3.png?raw=true) |
| ![Demo sugarloaf 4](https://github.com/raphamorim/rio/blob/main/sugarloaf/resources/demo-sugarloaf-4.png?raw=true) | ![Demo sugarloaf 5](https://github.com/raphamorim/rio/blob/main/sugarloaf/resources/demo-sugarloaf-5.png?raw=true) |
| ![Demo sugarloaf 6](https://github.com/raphamorim/rio/blob/main/sugarloaf/resources/demo-sugarloaf-6.png?raw=true) | |

## WASM Tests

### Setup

Install `wasm-bindgen-cli` globally: `cargo install wasm-bindgen-cli`.
`wasm-bindgen-cli` provides a test runner harness.

### Running Tests

Run (in the root sugarloaf directory):

```
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner cargo test --target wasm32-unknown-unknown -p sugarloaf --tests
```

Flag explanation:

- `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner`: Tells
  Cargo to use the test harness provided by `wasm-bindgen-cli`.
- `-p sugarloaf`: Only run tests in the sugarloaf directory.
- `--tests`: Only run tests; do not build examples. Many (possibly all) of the
  examples in sugarloaf/examples currently do not compile to WASM because they
  use networking.
