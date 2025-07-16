---
title: 'FreeBSD'
language: 'en'
---

Installation options:

- [FreeBSD Ports](https://ports.freebsd.org/cgi/ports.cgi?query=rio-terminal&stype=all&sektion=all)

## Manual Pages

After installing Rio, you can optionally install manual pages for offline documentation:

```bash
# Install scdoc (required to build man pages)
pkg install scdoc

# Build and install man pages from source
git clone https://github.com/raphamorim/rio.git
cd rio/extra/man
make
sudo make install

# Access documentation
man rio                # Main Rio manual
man 5 rio             # Configuration file format
man 5 rio-bindings    # Key bindings reference
```
