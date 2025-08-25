---
title: 'Frequently Asked Questions'
language: 'en'
---

### I get errors about the terminal being unknown or opening the terminal failing or functional keys like arrow keys don't work?

The famous case of "'rio' unknown terminal type".

All these issues have the same root cause: the rio terminfo is not available. Common scenarios include:

1. **SSH connections**: The remote system doesn't have Rio's terminfo installed
2. **Older Linux distributions**: The system doesn't include Rio's terminfo
3. **Package conflicts**: On Debian 13+/Ubuntu 24.04+, conflicts with ncurses-term package

#### Solutions:

**For Debian 13+ / Ubuntu 24.04+ users:**
- Rio's terminfo is included in `ncurses-term` (6.5+)
- If you get installation conflicts, your system already has the terminfo
- Simply ensure ncurses-term is up to date: `sudo apt install ncurses-term`

**For other distributions:**
- Install terminfo on the remote/local machine following [install/terminfo](/docs/install/terminfo)
- Arch Linux users can install: `sudo pacman -S rio-terminfo`

**Quick workarounds:**
- Use a fallback TERM for SSH: `TERM=xterm-256color ssh user@host`
- Configure a different TERM in Rio's config file

### Colors scheme not working as intended with tmux

The reason it happens is because tmux is using 256 colors configuration, you need to enable it or stop using Rio term without tmux (in case want to keep with 256 colors).

1. In case your shell does not report this correctly, add the following to your `~/.${SHELL}rc`:

```sh
export COLORTERM=truecolor
```

2. Please also note you need to add true color override to your `.tmux.conf` file:

```sh
set -g default-terminal "rio"
set-option -ga terminal-overrides ",rio:Tc"
```

3. Optionally you can also use `screen-256color`

```sh
set -g default-terminal "screen-256color"
set-option -ga terminal-overrides ",screen-256color:Tc"
```