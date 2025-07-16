---
title: 'MacOS'
language: 'en'
---

You can download Rio terminal application for macOS platform:

- [Download Rio for macOS](https://github.com/raphamorim/rio/releases/latest/download/rio.dmg)

Alternatively you can install Rio through [Homebrew](https://brew.sh/)...

```sh
brew install --cask rio
```

...or [MacPorts](https://www.macports.org):

```sh
sudo port install rio
```

**For Homebrew:** remember to run a "brew update" in case Homebrew cannot find a rio cask to install.

**For MacPorts:** more details [here](https://ports.macports.org/port/rio/).

Canary versions for MacOS are not notarized, so if you want to install a canary version you need to download and install the canary app from [github.com/raphamorim/rio/releases](https://github.com/raphamorim/rio/releases) and then follow the steps below:

- Try to run, it will show a window explaining it cannot be opened because "Apple cannot check it for malicious software.", then click Ok.
- Open System Preferences and select "Security & Privacy".
- If the padlock in the bottom left is locked, click it and authenticate to unlock it.
- Next to the message explaining the app "was blocked from use because it is not from an identified developer," click "Open Anyway".
- Close System Preferences and run the app.
- A notice will reiterate the warning about an inability to check if it is malicious, click Open.

## Manual Pages

After installing Rio, you can optionally install manual pages for offline documentation:

```bash
# Install scdoc (required to build man pages)
brew install scdoc

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
