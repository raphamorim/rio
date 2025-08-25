---
title: 'Frequently Asked Questions'
language: 'en'
---

### I get errors about the terminal being unknown or opening the terminal failing or functional keys like arrow keys don't work?

The famous case of "'rio' unknown terminal type".

All these issues have the same root cause: the rio terminfo is not available. 

#### Common Scenarios

1. **SSH connections**: The remote system doesn't have Rio's terminfo installed
2. **Ubuntu 22.04 and older**: Rio's .deb packages don't include terminfo (to avoid conflicts), so manual installation is needed
3. **Debian 13+/Ubuntu 24.04+**: Terminfo is provided by the system's ncurses-term package

#### Solutions by Distribution

**For Ubuntu 22.04 and older Debian/Ubuntu:**
```bash
# Rio .deb packages don't include terminfo, install it manually:
curl -o rio.terminfo https://raw.githubusercontent.com/raphamorim/rio/main/misc/rio.terminfo
sudo tic -xe xterm-rio,rio rio.terminfo
rm rio.terminfo
```

**For Debian 13+ / Ubuntu 24.04+:**
- Terminfo is included in `ncurses-term` (6.5+)
- Simply ensure ncurses-term is installed: `sudo apt install ncurses-term`

**For Arch Linux:**
```bash
sudo pacman -S rio-terminfo
```

**For RPM-based distributions:**
- Rio's RPM packages include terminfo, no action needed

#### Quick Workarounds

- Use a fallback TERM for SSH: `TERM=xterm-256color ssh user@host`
- Configure a different TERM in Rio's config file
- Copy terminfo to remote hosts: `infocmp -a xterm-rio | ssh myserver tic -x -o ~/.terminfo /dev/stdin`

For more details, see [install/terminfo](/docs/install/terminfo).

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