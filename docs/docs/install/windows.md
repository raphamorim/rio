---
title: 'Windows'
language: 'en'
---

Note: Rio is only available for Windows 10 or later.

Prebuilt binaries for Windows:

- [Download MSI installer for x86_64](https://github.com/raphamorim/rio/releases/latest/download/rio-installer-x86_64.msi)
- [Download portable executable for x86_64](https://github.com/raphamorim/rio/releases/latest/download/rio-portable-x86_64.exe)
- [Download MSI installer for aarch64](https://github.com/raphamorim/rio/releases/latest/download/rio-installer-aarch64.msi)
- [Download portable executable for aarch64](https://github.com/raphamorim/rio/releases/latest/download/rio-portable-aarch64.exe)

- Using WinGet package manager:

```sh
winget install -e --id raphamorim.rio
```

- [Using Chocolatey package manager](https://community.chocolatey.org/packages/rio-terminal)

```sh
choco install rio-terminal
```

- Using MINGW package manager: [packages.msys2.org/base/mingw-w64-rio](https://packages.msys2.org/base/mingw-w64-rio)
- [Using scoop](https://scoop.sh/#/apps?q=rio))

```sh
scoop bucket add extras
scoop install rio
```

There's a few things to note about the installer and the portable version:

- The browser will ask if you want to keep the file, click "Keep" to save the installer/executable on your computer.
- When opening the file, Windows will give you a warning, click "More info" and then "Run anyway" to run the installer/executable.

If you want to change the default shell to the new PowerShell platform, change the following line in your config file (see [Configuration file](/docs/config) for more information):

```toml
shell = { program = "pwsh", args = ["--login"] }
```

You may want to use a specific GPU on your system, specially if you're on a laptop configuration, this can enable hardware acceleration and improve performance of the application.
To make Windows utilize a GPU for a specific application through Windows display settings, follow the instructions:

1. Simultaneously press the Windows key and the letter "i" on your keyboard to open Windows Settings.
2. Select System.
3. Choose the Display option.
4. Click on the Graphics setting link located at the bottom of the page.
5. Select the application from the list or press the Browse button, then select the executable file for the application.
6. Click on the Options button to display the GPU selection window.
7. Choose the GPU you want to prioritize for the selected application.
8. Click on the Save button.
