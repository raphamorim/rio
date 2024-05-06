---
title: 'Terminfo'
language: 'en'
---

To make sure Rio works correctly, the "rio" terminfo must be used. The rio terminfo will be picked up automatically if it is installed.

If the following command returns without any errors, the rio terminfo is already installed:

```sh
infocmp rio
```

If it is not present already, you can install it globally with the following command:

When cloned locally, from the root of the repository run `sudo tic -xe rio misc/rio.terminfo`

If the source code has not been cloned locally:

```sh
curl -o rio.terminfo https://raw.githubusercontent.com/raphamorim/rio/main/misc/rio.terminfo
sudo tic -xe rio rio.terminfo
rm rio.terminfo
```
