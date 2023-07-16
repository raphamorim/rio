---
layout: docs
class: docs
title: 'Documentation'
language: 'en'
---

## Plugins

Plugins in Rio terminal are powered by WebAssembly.

And what it means in pratical terms?

You can write your plugin in any programming language that you want.

Rio provides hooks and controlling functions to WASM modules that are loaded in initialization time. Let's take a look in a plugin written with JavaScript or Rust.

{% highlight rust %}
#[link(wasm_import_module = "Rio")]
extern "C" {
    fn render() -> bool;
}

#[export_name = "render"]
pub fn render() {
    
}
{% endhighlight %}