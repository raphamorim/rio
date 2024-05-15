# Rio terminal

🚧 ⚠️ Currently, Rio is in the process of a major rewrite to 0.1.x to bring more features and fix known issues of 0.0.x versions.

> tl;dr: Rio is a terminal built to run everywhere, as a native desktop applications by Rust or even in the browser powered by WebAssembly.

<img src="misc/logo.svg" alt="Rio terminal logo" width="320px" />

[![Packaging status](https://repology.org/badge/vertical-allrepos/rio-terminal.svg)](https://repology.org/project/rio-terminal/versions)

## Platforms

| Name | Details |
| --- | --- |
| MacOs _as desktop application_ | [Installation guide](https://raphamorim.io/rio/docs/0.0.x/install/macos) |
| Linux _as desktop application_ | [Installation guide](https://raphamorim.io/rio/docs/0.0.x/install/linux) |
| Windows _as desktop application_ | [Installation guide](https://raphamorim.io/rio/docs/0.0.x/install/windows) |
| Web Browser _(WebAssembly)_ | (Sugarloaf is ready but Rio still need to be ported) |

## Demo Gallery

| ![Demo rio](docs/static/assets/posts/0.0.11/demo-rio.png) | ![Demo tmux](docs/static/assets/demos/demo-tmux.png) |
| ----------- | ----------- |
| ![Demo linux x11](docs/static/assets/posts/0.0.15/demo-navigation-x11.png) | ![Demo linux wayland](docs/static/assets/posts/0.0.15/demo-navigation-wayland.png) |
| ![Demo Windows 10](docs/static/assets/posts/0.0.8/demo-windows-10.png) |<img src="docs/static/assets/demos/demo-emacs.png" alt="Demo emacs" width="500px"/> |
| ![Demo native tabs macos](docs/static/assets/posts/0.0.17/demo-native-tabs.png) | ![Demo error handling](docs/static/assets/posts/0.0.19/demo-error-handling.png) |

Note: Emojis are rendered with Noto Emoji.

## About

Documentation: https://raphamorim.io/rio

If you are using or want to help in any way please consider to donate via [Github Sponsors](https://github.com/sponsors/raphamorim).

Rio would not be possible without [few acknowledgements](#acknowledgements) and specially [Alacritty](https://github.com/alacritty/alacritty/), since a lot of Rio functionalities (e.g: ANSI parser, events, grid system) was originally written (and still uses a good amount) of Alacritty code.

## Supporting the Project

If you use and like Rio, please consider sponsoring it: your support helps to cover the fees required to maintain the project and to validate the time spent working on it!

* [![Sponsor Rio terminal](https://img.shields.io/github/sponsors/raphamorim?label=Sponsor%20Rio&logo=github&style=for-the-badge)](https://github.com/sponsors/raphamorim)
* [Patreon](https://patreon.com/raphamorim)

## Acknowledgments

- Alacritty 🥇
- Since version 0.0.22, Sugarloaf ported glyph-brush code which was originally written by @alexheretic and licensed under Apache-2.0 license 🥇
- Components text render was originally from https://github.com/hecrj/wgpu_glyph
- The legacy Rio logo was made using _Adobe Sketchbook_ on iPad (between versions 0.0.1 between 0.0.18).
- WA was built originally from a fork from [Macroquad](https://github.com/not-fl3/macroquad) which is licensed under MIT license.
- https://github.com/servo/core-foundation-rs/blob/d4ce710182f1756c9d874ab917283fe1a1b7a011/cocoa/src/appkit.rs#L1447
