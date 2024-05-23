---
title: 'MacOS'
language: 'en'
---


You can download Rio terminal application for macOS platform:

- [Download Rio for MacOS v0.0.38](https://github.com/raphamorim/rio/releases/download/v0.0.38/Rio-v0.0.38.dmg)

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
