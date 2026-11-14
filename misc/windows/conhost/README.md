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
checks its SHA-256 against the one pinned in the script, checks that
each file carries a valid Microsoft Authenticode signature, and lays out
one install tree per Rio build in `amd64/` and `arm64/` here, which git
ignores. The release pipeline runs it before packaging (`.goreleaser.yaml`), and
the ConPTY workflow runs it on every change to the script. Run it
yourself on Windows to build a package locally; elsewhere it checks
the hash and lays out nothing, since signatures cannot be checked
there. The binaries keep Microsoft's
Authenticode signature and must not be re-signed. `LICENSE` here is
the project's MIT notice and ships beside the files as
`LICENSE-ConPTY.txt`, as the license requires.

## Layout

- `conpty.dll` sits next to `rio.exe` and matches its architecture.
  Rio loads it from that directory only, by full path; a copy on `PATH`
  or in the working directory is never used, and without one Rio falls
  back to the Windows API. The log line `Using conpty.dll for
  pseudoconsole` confirms the bundled host is in use; a DLL that is
  there but cannot be loaded (wrong architecture, say) is logged with
  the reason. A bare `rio.exe` that used to find a `conpty.dll` on
  `PATH` no longer does: the portable zip carries the files instead.
- `OpenConsole.exe` matches the machine, not the application.
  `conpty.dll` looks for it beside itself, then in an `x64/` or
  `arm64/` folder named for the machine, and without one it silently
  falls back to the in-box `conhost.exe`. Rio ships the host in those
  folders: the x64 build carries both, since it also runs on ARM64
  machines; the arm64 build carries `arm64/` only.
- The installer puts `C:\Program Files\Rio` first on the system
  PATH, so a program that loads `conpty.dll` by name and ships none of
  its own may pick up Rio's copy. That gives it the same modern host,
  and it falls back to the Windows API when Rio is uninstalled.

## Updating

1. Pick a version from the package's version list. Stable versions have
   no `-preview` suffix.
2. In `fetch.ps1`, set `$Version`, then run
   `pwsh misc/windows/conhost/fetch.ps1 -SkipHash` and copy the printed
   SHA-256 into `$Sha256`.
3. Run the script again without `-SkipHash`, build the installer or zip,
   and check on a Windows machine that images render in a program such
   as yazi and that leaving a TUI restores the screen.

Pin nothing below 1.25.260930003. Early 1.22 builds did not restore
the main screen when a TUI exited and repainted stale content on
resize (microsoft/terminal #17817, #16879); 1.24.260710001 still broke
yazi previews for one reporter; 1.25.260930003 and later fixed every
report so far.
