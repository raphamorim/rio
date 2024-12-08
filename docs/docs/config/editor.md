---
title: 'editor'
language: 'en'
---

This setting specifies the editor Rio will use to open the configuration file. By default, the editor is set to `vi`.

Whenever the key binding `OpenConfigEditor` is triggered, Rio will use the configured editor and the path to the Rio configuration file.

For example, if you have VS Code installed and want to use it as your editor, the configuration would look like this:

```toml
[editor]
program = "code"
args = []
```

When `OpenConfigEditor` is triggered, it will execute the command:
`$ code <path-to-rio-configuration-file>`.

:::warning

If you set a value for `program`, Rio will look for it in the default system application directory (`/usr/bin` on Linux and macOS). If your desired editor is not in this directory, you must specify its full path:

```toml
[editor]
program = "/usr/local/bin/code"
args = []
```

:::