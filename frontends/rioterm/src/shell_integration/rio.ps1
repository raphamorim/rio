# Reports the working directory to rio (OSC 7, kitty-shell-cwd flavor:
# the path travels verbatim, so spaces, `#` and `%` need no encoding),
# which powers the title path variables and opening new tabs in the
# current directory. UNC paths are skipped: the verbatim flavor has no
# host field to mark them as belonging to another machine. Loading
# twice is a no-op.
if ($Global:__RioShellIntegration) { return }
$Global:__RioShellIntegration = $true

$Global:__RioOriginalPrompt = $Function:Prompt

function Global:Prompt {
    # Preserve the state the user's prompt inspects: $LASTEXITCODE is
    # restored directly, and since $? is not assignable, a deliberately
    # failing statement re-arms it when the last command failed.
    $Global:__RioLastSuccess = $?
    $Global:__RioLastExitCode = $Global:LASTEXITCODE
    $location = $ExecutionContext.SessionState.Path.CurrentLocation
    if ($location.Provider.Name -eq 'FileSystem') {
        $path = $location.ProviderPath
        if (-not $path.StartsWith('\\')) {
            if (-not $path.StartsWith('/')) { $path = "/$path" }
            [Console]::Write("$([char]27)]7;kitty-shell-cwd://$path$([char]7)")
        }
    }
    $Global:LASTEXITCODE = $Global:__RioLastExitCode
    if ($Global:__RioLastSuccess) {
        & $Global:__RioOriginalPrompt
    } else {
        Microsoft.PowerShell.Utility\Write-Error '' -ErrorAction SilentlyContinue
        & $Global:__RioOriginalPrompt
    }
}
