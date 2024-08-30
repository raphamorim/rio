---
title: 'keyboard'
language: 'en'
---

- `use-kitty-keyboard-protocol` - Enable Kitty Keyboard protocol

- `disable-ctlseqs-alt` - Disable ctlseqs with ALT keys
  - Useful for example if you would like Rio to replicate Terminal.app, since it does not deal with ctlseqs with ALT keys

Example:

```toml
[keyboard]
use-kitty-keyboard-protocol = false
disable-ctlseqs-alt = false
```