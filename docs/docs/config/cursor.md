---
title: 'cursor'
language: 'en'
---

By default, the cursor shape is set to `block`. You can also choose from other options like `underline` and `beam`.

Additionally, you can enable or disable cursor blinking, which is set to `false` by default.

### Shape

Options: 'block', 'underline', 'beam'

```toml
[cursor]
shape = 'block'
```

### Blinking

Enable/disable blinking (default: false)

```toml
[cursor]
blinking = false
```

### Blinking-interval

Set cursor blinking interval (default: 800, only configurable from 350ms to 1200ms).

```toml
[cursor]
blinking-interval = 800
```
