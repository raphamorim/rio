---
title: 'Configuration file'
language: 'en'
---

The configuration should be the following paths otherwise Rio will use the default configuration.

MacOS and Linux configuration file path is `~/.config/rio/config.toml`.

Windows configuration file path is `C:\Users\USER\AppData\Local\rio\config.toml` (replace "USER" with your user name).

Updates to the configuration file automatically triggers Rio to render the terminal with the new configuration.

Note that all parameters without a header must be at the beginning of the file, otherwise they will be ignored. Example:

```toml
[editor]
program = "vi"
args = []

theme = "dracula" # ignore it, be under the `editor` header
```

```toml
theme = "dracula" # it works, be without heading

[editor]
program = "vi"
args = []
```
