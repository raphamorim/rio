#!/usr/bin/env pwsh
# Fetches the ConPTY pair Rio ships beside rio.exe on Windows:
# conpty.dll and OpenConsole.exe from the Windows Terminal project,
# published by Microsoft as the Microsoft.Windows.Console.ConPTY NuGet
# package (MIT). The release pipeline runs this before packaging; the
# files land in amd64/ and arm64/ next to this script, one install
# layout per Rio build, which git ignores. See README.md here for why
# and how to update.
#
# To bump: set $Version to the new package version, run the script
# with -SkipHash to download it, and put the printed SHA-256 in $Sha256.
# Off Windows the script stops after the hash: files are laid out only
# where their signatures can be checked (the conpty workflow does it).
[CmdletBinding()]
param(
    [string]$Version = "1.25.261109001",
    [string]$Sha256 = "3d77f535df848e44225c2a8912ac844fecb23f3057732b639f3034b587496f09",
    [switch]$SkipHash
)

$ErrorActionPreference = "Stop"

$here = $PSScriptRoot
$package = "microsoft.windows.console.conpty"
$url = "https://api.nuget.org/v3-flatcontainer/$package/$Version/$package.$Version.nupkg"
$work = Join-Path $here "nupkg"
# Expand-Archive only opens .zip, and a .nupkg is a zip.
$archive = Join-Path $work "conpty.zip"

New-Item -ItemType Directory -Force -Path $work | Out-Null
Write-Host "Downloading $url"
Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing `
    -MaximumRetryCount 3 -RetryIntervalSec 5

$hash = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
if ($SkipHash) {
    Write-Host "SHA-256 $hash"
} elseif ($hash -ne $Sha256.ToLowerInvariant()) {
    throw "SHA-256 of $archive is $hash, expected $Sha256"
}

# Get-AuthenticodeSignature exists on Windows only, and no package is
# built anywhere else, so elsewhere the script ends here.
if ($env:OS -ne "Windows_NT") {
    Remove-Item -Recurse -Force $work
    Write-Host "Not on Windows: signatures cannot be checked, nothing laid out"
    exit 0
}

$extracted = Join-Path $work "x"
if (Test-Path $extracted) { Remove-Item -Recurse -Force $extracted }
Expand-Archive -Path $archive -DestinationPath $extracted

# conpty.dll matches the application. OpenConsole.exe matches the
# machine: conpty.dll looks for it in a subfolder named for the
# machine's architecture. An x64 Rio also runs on ARM64 machines, so
# it carries both hosts; an arm64 Rio only runs on ARM64.
$files = @(
    @{ src = "runtimes/win-x64/native/conpty.dll";          dst = "amd64/conpty.dll" },
    @{ src = "build/native/runtimes/x64/OpenConsole.exe";   dst = "amd64/x64/OpenConsole.exe" },
    @{ src = "build/native/runtimes/arm64/OpenConsole.exe"; dst = "amd64/arm64/OpenConsole.exe" },
    @{ src = "runtimes/win-arm64/native/conpty.dll";        dst = "arm64/conpty.dll" },
    @{ src = "build/native/runtimes/arm64/OpenConsole.exe"; dst = "arm64/arm64/OpenConsole.exe" }
)
# The hash pins the package; the signature check is independent of it,
# so a bump done with -SkipHash still refuses anything Microsoft did
# not sign. The signer must be Microsoft Corporation itself: Microsoft
# also runs certificates that sign other vendors' code, and those
# carry a different CN under the same O.
foreach ($file in $files) {
    $dst = Join-Path $here $file.dst
    New-Item -ItemType Directory -Force -Path (Split-Path $dst) | Out-Null
    Copy-Item -Force (Join-Path $extracted $file.src) $dst
    $signature = Get-AuthenticodeSignature $dst
    if ($signature.Status -ne "Valid" -or
        $signature.SignerCertificate.Subject -notmatch "(^|, )CN=Microsoft Corporation(,|$)") {
        throw "$($file.dst) is not signed by Microsoft Corporation (status $($signature.Status))"
    }
    Write-Host "  $($file.dst)"
}

Remove-Item -Recurse -Force $work
Write-Host "ConPTY $Version ready"
