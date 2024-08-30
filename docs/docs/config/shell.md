---
title: 'shell'
language: 'en'
---

You can set `shell.program` to the path of your favorite shell, e.g. `/bin/fish`.

Entries in `shell.args` are passed unmodified as arguments to the shell.

Default:

- (macOS) user login shell
- (Linux/BSD) user login shell
- (Windows) powershell

### Shell Examples

1. MacOS using fish shell from bin path:

```toml
[shell]
program = "/bin/fish"
args = ["--login"]
```

2. Windows using powershell:

```toml
[shell]
program = "pwsh"
args = []
```

3. Windows using powershell with login:

```toml
[shell]
program = "pwsh"
args = ["-l"]
```

4. MacOS with tmux installed by homebrew:

```toml
[shell]
program = "/opt/homebrew/bin/tmux"
args = ["new-session", "-c", "/var/www"]
```