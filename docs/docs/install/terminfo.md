---
title: 'Terminfo'
language: 'en'
---

To make sure Rio works correctly, the Rio terminfo must be used. The rio terminfo will be picked up automatically if it is installed.

If the following command returns without any errors, the xterm-rio terminfo is already installed:

```sh
infocmp rio
```

If it is not present already, you can install it globally with the following command:

When cloned locally, from the root of the repository run `sudo tic -xe xterm-rio,rio misc/rio.terminfo`

If the source code has not been cloned locally:

```sh
curl -o rio.terminfo https://raw.githubusercontent.com/raphamorim/rio/main/misc/rio.terminfo
sudo tic -xe xterm-rio,rio rio.terminfo
rm rio.terminfo
```

## Terminfo Entries

The Rio terminfo provides two entries:

- `xterm-rio`: A compatibility entry for applications that look for "xterm-" prefixed terminals to detect capabilities
- `rio`: The terminfo entry that was used by the `TERM` environment variable until v0.2.27

Both entries provide the same terminal capabilities, ensuring compatibility with a wide range of applications.
