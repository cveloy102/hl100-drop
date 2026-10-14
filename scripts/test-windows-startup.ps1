param([string]$Executable = "$PSScriptRoot/../artifacts/windows/HL100Drop.exe")
$ErrorActionPreference = 'Stop'
$taskExecutable = (Resolve-Path -LiteralPath $Executable).Path
$taskRoot = Split-Path $PSScriptRoot -Parent
$taskQa = Join-Path $taskRoot ("artifacts/qa/startup-"+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $taskQa | Out-Null
$taskSettings = Join-Path $taskQa 'settings.json'
@{Id=[Guid]::NewGuid().ToString('N');Name='Startup regression';Folder=$taskQa;Configured=$true;CheckUpdates=$false;Notifications=$false} | ConvertTo-Json | Set-Content -Encoding utf8NoBOM -LiteralPath $taskSettings
$taskPreviousSettings = $env:HL100_SETTINGS_PATH
$taskBackground = $null
$taskManual = $null
try {
    $env:HL100_SETTINGS_PATH = $taskSettings
    $taskBackground = Start-Process -FilePath $taskExecutable -ArgumentList '--login' -PassThru
    $taskDeadline = [DateTime]::UtcNow.AddSeconds(20)
    $taskListening = $false
    do {
        Start-Sleep -Milliseconds 200
        $taskBackground.Refresh()
        if ($taskBackground.HasExited) { throw 'Background login exited before reception started' }
        if ($taskBackground.MainWindowHandle -ne [IntPtr]::Zero) { throw 'Background login displayed a window' }
        $taskListening = [bool](Get-NetTCPConnection -State Listen -LocalPort 41000 -ErrorAction SilentlyContinue | Where-Object OwningProcess -eq $taskBackground.Id)
    } while (!$taskListening -and [DateTime]::UtcNow -lt $taskDeadline)
    if (!$taskListening) { throw 'Hidden login did not start the receiving listener' }
    # A second manual launch must signal the existing instance, not start another listener.
    $taskManual = Start-Process -FilePath $taskExecutable -PassThru
    if (!$taskManual.WaitForExit(10000) -or $taskManual.ExitCode -ne 0) { throw 'Manual launcher did not hand off to the existing instance' }
    $taskDeadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        Start-Sleep -Milliseconds 100
        $taskBackground.Refresh()
    } while (!$taskBackground.HasExited -and $taskBackground.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $taskDeadline)
    if ($taskBackground.HasExited -or $taskBackground.MainWindowHandle -eq [IntPtr]::Zero) { throw 'Manual launch did not open the existing window' }
    Write-Output 'PASS: hidden login keeps receiving; manual launch opens the same instance'
} finally {
    foreach ($taskProcess in @($taskManual,$taskBackground)) {
        if ($null -ne $taskProcess) { $taskProcess.Refresh(); if (!$taskProcess.HasExited) { Stop-Process -Id $taskProcess.Id } }
    }
    if ($null -eq $taskPreviousSettings) { Remove-Item Env:HL100_SETTINGS_PATH -ErrorAction SilentlyContinue }
    else { $env:HL100_SETTINGS_PATH = $taskPreviousSettings }
}
