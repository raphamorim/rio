---
title: 'Kitty keyboard protocol'
language: 'en'
---

Rio terminal implements Kitty keyboard protocol. It is disabled by default, for activate you need to set the configuration as true.

```toml
use-kitty-keyboard-protocol = true
```

### How it works?

There are various problems with the current state of keyboard handling in terminals. They include:

- No way to use modifiers other than ctrl and alt

- No way to reliably use multiple modifier keys, other than, shift+alt and ctrl+alt.

- Many of the existing escape codes used to encode these events are ambiguous with different key presses mapping to the same escape code.

- No way to handle different types of keyboard events, such as press, release or repeat

- No reliable way to distinguish single Esc key presses from the start of a escape sequence. Currently, client programs use fragile timing related hacks for this, leading to bugs, for example: [neovim #2035](https://github.com/neovim/neovim/issues/2035).

To solve these issues and others, kitty has created a new keyboard protocol, that is backward compatible but allows applications to opt-in to support more advanced usages.

