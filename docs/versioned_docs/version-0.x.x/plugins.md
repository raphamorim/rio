---
title: 'Plugins'
language: 'en'
---

## Note: Plugins are not ready yet

Plugins in Rio terminal are powered by WebAssembly. Which means they can be written in any programming language, as long as it is able to be compiled to WebAssembly.

Rio provides hooks and controlling functions to WASM modules that are loaded in initialization time. Let's take a look in a plugin written with JavaScript or Rust.

```rust
#[link(wasm_import_module = "Rio")]
extern "C" {
    fn render() -> bool;
}

#[export_name = "render"]
pub fn render() {

}
```
