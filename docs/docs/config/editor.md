---
title: 'editor'
language: 'en'
---

Default editor is `vi`.

Whenever the key binding `OpenConfigEditor` is triggered it will use the value of the editor along with the rio configuration path.

An example, considering you have VS Code installed and you want to use it as your editor:

```toml
[editor]
program = "code"
args = []
```

Whenever `OpenConfigEditor` runs it will trigger `$ code <path-to-rio-configuration-file>`.