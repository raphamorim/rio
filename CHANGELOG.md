# Rio Changelog

## 0.0.2

- `log-level` as configurable (`DEBUG`, `INFO`, `TRACE`, `ERROR`, `WARN` and `OFF`). `OFF` by default.
- Introduction of rendering engine called Sugarloaf.
- System font loader (tested and implemented for MacOs).

## 0.0.1

- Basic move/goto functionalities.
- Initial definition of Rio default colors.
- Set and reset color by ANSI parser.
- Clear/Tabs functionalities.
- Grid introduction.
- Desktop delta scroll (up and down, without scrollbar UI component).
- `Teletypewriter` 2.0.0 usage for macos and linux.
- Resize support.
- $SHELL login on macos, by default: `/bin/zsh --login` (if $SHELL is settled as other could as run `/bin/bash --login`, `/bin/fish --login` ...)
- Cursor initial support (without VI mode).
