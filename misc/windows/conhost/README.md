# Bundled ConPTY

Rio's Windows installer and portable zip ship `conpty.dll` and
`OpenConsole.exe` from the Windows Terminal project next to `rio.exe`,
so Rio drives a modern pseudoconsole instead of the one built into
Windows. The in-box ConPTY on most Windows builds predates 1.22 and
rewrites what programs print: image protocols (sixel, iTerm2, kitty
graphics) never reach Rio, and the cursor position a TUI leaves behind
is lost, which puts the IME candidate window in the wrong place. The
modern host passes the output through unchanged (rio#1759, rio#2031).

The files are not in the repository. `fetch.ps1` downloads the
`Microsoft.Windows.Console.ConPTY` NuGet package (MIT, published by
Microsoft, <https://www.nuget.org/packages/Microsoft.Windows.Console.ConPTY>),
checks its SHA-256 against the one pinned in the script, and puts the
four files in `amd64/` and `arm64/` here, which git ignores. The
release pipeline runs it before packaging (`.goreleaser.yaml`); run it
yourself to build a package locally. The binaries keep Microsoft's
Authenticode signature and must not be re-signed.

## Layout

- `conpty.dll` matches the application architecture, `OpenConsole.exe`
  the machine's. Each Rio build ships the pair of its own
  architecture: an x64 Rio on an ARM64 machine runs its x64
  `OpenConsole.exe` emulated, which works.
- `conpty.dll` looks for `OpenConsole.exe` in its own directory, then
  in an `x64/` or `arm64/` subdirectory named for the machine, and
  without one it silently falls back to the in-box `conhost.exe`, so
  the two files must always travel together.
- Rio loads `conpty.dll` by name, so the Windows loader looks in
  `rio.exe`'s directory first and falls back to the Windows API when
  there is none. The log line `Using conpty.dll for pseudoconsole`
  confirms the bundled host is in use.

## Updating

1. Pick a version from the package's version list. Stable versions have
   no `-preview` suffix.
2. In `fetch.ps1`, set `$Version`, then run
   `pwsh misc/windows/conhost/fetch.ps1 -SkipHash` and copy the printed
   SHA-256 into `$Sha256`.
3. Run the script again without `-SkipHash`, build the installer or zip,
   and check on a Windows machine that images render in a program such
   as yazi and that leaving a TUI restores the screen.

Versions below 1.24.2607 are known bad: early 1.22 builds did not
restore the main screen when a TUI exited and repainted stale content
on resize (microsoft/terminal #17817, #16879), and 1.24.260710001 still
broke yazi previews for one reporter. 1.25.260930003 and later fixed
every report so far.
