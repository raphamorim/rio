# Reports the working directory to rio (OSC 7) from the prompt, which
# powers the title path variables and opening new tabs in the current
# directory. Loading twice is a no-op.
if ($Global:__RioShellIntegration) { return }
$Global:__RioShellIntegration = $true

$Global:__RioOriginalPrompt = $Function:Prompt

function Global:Prompt {
    $location = $ExecutionContext.SessionState.Path.CurrentLocation
    if ($location.Provider.Name -eq 'FileSystem') {
        $uri = ([System.Uri]$location.ProviderPath).AbsoluteUri
        [Console]::Write("$([char]27)]7;$uri$([char]7)")
    }
    & $Global:__RioOriginalPrompt
}
