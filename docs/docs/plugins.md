---
title: 'Plugins'
language: 'en'
---

Rio can support any plugin written in WASM or Lua directly.

Plugins works in Rio based on events. Let's say you would like to create an [ChatGPT](https://openai.com/index/chatgpt/) or any AI plugin that would integrate with Rio terminal.

In the plugin you can specify how it would work, let's write one in Rust:

```rs


```

- `on_keyup()`
- `on_keydown()`
- `on_render()`
- `request_render()`
- `append_element()`