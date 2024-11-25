---
title: 'platform'
language: 'en'
---

Rio allows you to have different configurations per OS, you can write ovewrite `Shell`, `Navigation`, `Renderer` and `Window`.

Example:

```toml
[shell]
# default (in this case will be used only on MacOS)
program = "/bin/fish"
args = ["--login"]

[platform]
# Microsoft Windows overwrite
windows.shell.program = "pwsh"
windows.shell.args = ["-l"]

# Linux overwrite
linux.shell.program = "tmux"
linux.shell.args = ["new-session", "-c", "/var/www"]
```