param([string]$Version = '1.1.3', [switch]$Stable)
. "$PSScriptRoot/environment.ps1"
$taskRepo = 'cveloy102/hl100-drop'
$taskFiles = @("HL100DropSetup-$Version.exe", "HL100Drop-$Version.apk", "HL100Drop-$Version-universal.dmg")
foreach ($taskFile in $taskFiles) { if (!(Test-Path (Join-Path $taskRoot "artifacts/$taskFile"))) { throw "Missing release asset: $taskFile" } }
$taskChecksums = foreach ($taskFile in $taskFiles) {
    $taskHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $taskRoot "artifacts/$taskFile")).Hash.ToLowerInvariant()
    "$taskHash  $taskFile"
}
# UTF-8 without BOM in both Windows PowerShell 5.1 and PowerShell 7.
[IO.File]::WriteAllText((Join-Path $taskRoot 'artifacts/checksums.txt'), (($taskChecksums -join "`n") + "`n"))
# Use the existing GitHub CLI or Git Credential Manager login only for the user's repository.
# Credential output is captured in memory and is never logged or written to disk.
$taskToken = $null
if (Get-Command gh -ErrorAction SilentlyContinue) {
    $taskToken = (& gh auth token -h github.com 2>$null | Select-Object -First 1)
    if ($LASTEXITCODE -ne 0) { $taskToken = $null }
}
if (!$taskToken) {
    # Write stdin directly: Windows PowerShell 5.1 mangles multi-line text piped to native commands.
    $taskStart = New-Object System.Diagnostics.ProcessStartInfo 'git', 'credential fill'
    $taskStart.UseShellExecute = $false; $taskStart.RedirectStandardInput = $true; $taskStart.RedirectStandardOutput = $true
    $taskProcess = [System.Diagnostics.Process]::Start($taskStart)
    $taskProcess.StandardInput.Write("protocol=https`nhost=github.com`n`n"); $taskProcess.StandardInput.Close()
    $taskCredential = $taskProcess.StandardOutput.ReadToEnd() -split "`n"; $taskProcess.WaitForExit()
    $taskTokenLine = $taskCredential | Where-Object { $_.StartsWith('password=') } | Select-Object -First 1
    if ($taskTokenLine) { $taskToken = $taskTokenLine.Substring(9).Trim() }
}
if (!$taskToken) { throw 'Sign in with gh auth login or Git Credential Manager before publishing' }
$taskHeaders = @{ Authorization='Bearer '+$taskToken; Accept='application/vnd.github+json'; 'X-GitHub-Api-Version'='2022-11-28'; 'User-Agent'='HL100Drop-release' }
try {
    $taskBranch = git -C $taskRoot -c "safe.directory=$taskRoot" branch --show-current
    $taskBody = @{ tag_name="v$Version";target_commitish=$taskBranch;name="HL100 Drop $Version";body=[IO.File]::ReadAllText((Join-Path $taskRoot 'scripts/release-notes.md'));prerelease=(!$Stable);draft=$true } | ConvertTo-Json
    # Send bytes: Windows PowerShell 5.1 would encode a string body as ISO-8859-1 and garble Russian notes.
    $taskRelease = Invoke-RestMethod -Method Post -Uri "https://api.github.com/repos/$taskRepo/releases" -Headers $taskHeaders -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($taskBody))
    foreach ($taskFile in ($taskFiles+@('checksums.txt'))) {
        $taskUrl = "https://uploads.github.com/repos/$taskRepo/releases/$($taskRelease.id)/assets?name=$([Uri]::EscapeDataString($taskFile))"
        Invoke-RestMethod -Method Post -Uri $taskUrl -Headers $taskHeaders -ContentType 'application/octet-stream' -InFile (Join-Path $taskRoot "artifacts/$taskFile") | Out-Null
    }
    $taskUploaded = Invoke-RestMethod -Uri "https://api.github.com/repos/$taskRepo/releases/$($taskRelease.id)/assets" -Headers $taskHeaders
    foreach ($taskFile in ($taskFiles+@('checksums.txt'))) {
        $taskAsset = $taskUploaded | Where-Object name -eq $taskFile | Select-Object -First 1
        $taskExpected = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $taskRoot "artifacts/$taskFile")).Hash.ToLowerInvariant()
        if (!$taskAsset -or $taskAsset.digest -ne "sha256:$taskExpected") { throw "Release asset verification failed: $taskFile; draft retained" }
    }
    $taskRelease = Invoke-RestMethod -Method Patch -Uri "https://api.github.com/repos/$taskRepo/releases/$($taskRelease.id)" -Headers $taskHeaders -ContentType 'application/json' -Body '{"draft":false}'
    Write-Output $taskRelease.html_url
} finally { $taskHeaders.Clear();$taskCredential=$null;$taskTokenLine=$null;$taskToken=$null }
