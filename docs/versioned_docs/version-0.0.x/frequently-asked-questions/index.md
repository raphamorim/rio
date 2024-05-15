---
title: 'Frequently Asked Questions'
language: 'en'
---

### I get errors about the terminal being unknown or opening the terminal failing or functional keys like arrow keys don’t work?

The famous case of "'rio' unknown terminal type".

All the issues all have the same root cause: the rio terminfo is not available. The most common way this happens is SSHing into a computer that does not have the rio terminfo files. The simplest fix would be install the terminfo files on the remote machine (following terminfo instruction in install section [install/terminfo](/docs/0.0.x/install/terminfo))

Other alternative is use a different terminfo either in config or per connection with environment variable like `TERM=xterm-256color ssh`.

If you are using a well maintained Linux distribution, it will have a rio-terminfo package that you can simply install to make the rio terminfo files available system-wide. Then the problem will no longer occur. Arch Linux for example [rio-terminfo](https://archlinux.org/packages/extra/x86_64/rio-terminfo/)

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
