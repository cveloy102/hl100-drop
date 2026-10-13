param([string]$Version = '1.1.1', [switch]$Stable)
. "$PSScriptRoot/environment.ps1"
$taskRepo = 'cveloy102/hl100-drop'
$taskFiles = @("HL100DropSetup-$Version.exe", "HL100Drop-$Version.apk", "HL100Drop-$Version-universal.dmg")
foreach ($taskFile in $taskFiles) { if (!(Test-Path (Join-Path $taskRoot "artifacts/$taskFile"))) { throw "Missing release asset: $taskFile" } }
$taskChecksums = foreach ($taskFile in $taskFiles) {
    $taskHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $taskRoot "artifacts/$taskFile")).Hash.ToLowerInvariant()
    "$taskHash  $taskFile"
}
$taskChecksums | Set-Content -Encoding utf8NoBOM -LiteralPath (Join-Path $taskRoot 'artifacts/checksums.txt')
# Use the existing Git Credential Manager login only for the user's GitHub repository.
# Credential output is captured in memory and is never logged or written to disk.
$taskCredential = "protocol=https`nhost=github.com`n`n" | git credential fill
Assert-Exit 'GitHub credential lookup'
$taskTokenLine = $taskCredential | Where-Object { $_.StartsWith('password=') } | Select-Object -First 1
if (!$taskTokenLine) { throw 'Sign in using Git Credential Manager before publishing' }
$taskHeaders = @{ Authorization='Bearer '+$taskTokenLine.Substring(9); Accept='application/vnd.github+json'; 'X-GitHub-Api-Version'='2022-11-28'; 'User-Agent'='HL100Drop-release' }
try {
    $taskBranch = git -C $taskRoot -c "safe.directory=$taskRoot" branch --show-current
    $taskBody = @{ tag_name="v$Version";target_commitish=$taskBranch;name="HL100 Drop $Version";body=(Get-Content -Raw (Join-Path $taskRoot 'scripts/release-notes.md'));prerelease=(!$Stable);draft=$true } | ConvertTo-Json
    $taskRelease = Invoke-RestMethod -Method Post -Uri "https://api.github.com/repos/$taskRepo/releases" -Headers $taskHeaders -ContentType 'application/json' -Body $taskBody
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
} finally { $taskHeaders.Clear();$taskCredential=$null;$taskTokenLine=$null }
