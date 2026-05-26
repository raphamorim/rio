# Sugarloaf

Sugarloaf is Rio rendering engine, designed to be multiplatform. It is based on WebGPU, Rust library for Desktops and WebAssembly for Web (JavaScript). This project is created and maintained for Rio terminal purposes but feel free to use it.

```bash
cargo run --example text
```

## Build dependencies

### Linux — Vulkan backend

The native Vulkan backend (default on Linux) compiles its GLSL shaders to SPIR-V at build time. You need one GLSL → SPIR-V compiler installed on the build host:

| Distro | Command |
|---|---|
| Debian / Ubuntu | `apt install glslang-tools` (or `apt install glslc`) |
| Arch | `pacman -S shaderc` (provides `glslc`) |
| Fedora | `dnf install glslang` (or `dnf install glslc`) |

`glslc` is preferred when both are present. Override with `GLSLC=/path/to/binary` or `GLSLANG_VALIDATOR=/path/to/binary`.

The compiled SPIR-V lives in `OUT_DIR` per build — the source `.glsl` files are checked in but the `.spv` artifacts are gitignored.

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
