# Bundled ConPTY

`conpty.dll` and `OpenConsole.exe` from the Windows Terminal project,
redistributed so Rio uses a modern pseudoconsole on Windows instead of
the older in-box one. The modern ConPTY (>= 1.22) passes terminal VT
through unmodified, which is what makes image protocols (sixel, iTerm2,
kitty graphics) work, and it fixes the alt-screen-restore and resize
regressions that the early builds had (see rio#1759, microsoft/terminal
#17817 / #17853 / #16879).

Both files must sit next to `rio.exe`. Rio loads `conpty.dll` by absolute
path from its own executable directory only — never from `PATH` or the
working directory — so a copy placed anywhere else is ignored (Rio falls
back to the in-box API). And `conpty.dll` locates its `OpenConsole.exe`
host in its own directory, so without a matching `OpenConsole.exe` beside
it, it silently reverts to the in-box console host.
`conpty.dll` matches the application architecture; `OpenConsole.exe`
matches the system architecture. The `amd64/` and `arm64/` folders hold
the matched pair for each Rio build.

- Source: `Microsoft.Windows.Console.ConPTY` NuGet package
  <https://www.nuget.org/packages/Microsoft.Windows.Console.ConPTY>
- Version: 1.24.260710001
- License: MIT (Windows Terminal, <https://github.com/microsoft/terminal>)

## Updating

Download the `.nupkg` (a zip), then copy:
- `runtimes/win-x64/native/conpty.dll`   -> `amd64/conpty.dll`
- `build/native/runtimes/x64/OpenConsole.exe` -> `amd64/OpenConsole.exe`
- `runtimes/win-arm64/native/conpty.dll` -> `arm64/conpty.dll`
- `build/native/runtimes/arm64/OpenConsole.exe` -> `arm64/OpenConsole.exe`

Keep `conpty.dll` and `OpenConsole.exe` from the same package version.
