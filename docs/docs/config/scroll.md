---
title: 'scroll'
language: 'en'
---

You can change how many lines are scrolled each time by setting this option. Scroll calculation for canonical mode will be based on `lines = (accumulated scroll * multiplier / divider)`.

If you want a quicker scroll, keep increasing the multiplier. If you want to reduce scroll speed you will need to increase the divider.

You can combine both properties to find the best scroll for you.

- Multiplier default is `3.0`.
- Divider default is `1.0`.

Example:

```toml
[scroll]
multiplier = 3.0
divider = 1.0
```
