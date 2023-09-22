---
title: 'Terminfo'
language: 'en'
---

To make sure Rio works correctly, the "rio" terminfo must be used. The rio terminfo will be picked up automatically if it is installed.

If the following command returns without any errors, the rio terminfo is already installed:

```bash
infocmp rio
```

If it is not present already, you can install it globally with the following command:

```bash
sudo tic -xe rio misc/rio.terminfo
```
