# Rio terminal

> tl;dr: Rio is a terminal built to run everywhere, as a native desktop applications by Rust or even in the browser powered by WebAssembly.

> [!NOTE]
> The `v0.1.x` releases are yet unstable compared to `v0.0.x`. If any new release from `v0.1.x` doesn't work for you, it is highly recommended to keep with `v0.0.x` versions until the issue is fixed.

<img src="misc/logo.svg" alt="Rio terminal logo" width="320px" />

[![Packaging status](https://repology.org/badge/vertical-allrepos/rio-terminal.svg)](https://repology.org/project/rio-terminal/versions)

## Platforms

| Name | Details |
| --- | --- |
| MacOs _as desktop application_ | [Installation guide](https://raphamorim.io/rio/docs/0.0.x/install/macos) |
| Linux _as desktop application_ | [Installation guide](https://raphamorim.io/rio/docs/0.0.x/install/linux) |
| Windows _as desktop application_ | [Installation guide](https://raphamorim.io/rio/docs/0.0.x/install/windows) |
| Web Browser _(WebAssembly)_ | (Sugarloaf is ready but Rio still need to be ported) |

Demo usage of Rio terminal on MacOS using [Fira Code](https://fonts.google.com/specimen/Fira+Code) and font ligatures enabled:

![Demo Rio on MacOS](docs/static/assets/posts/0.1.0/demo-rio.png)

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
