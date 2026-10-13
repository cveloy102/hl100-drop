param([string]$Address = '127.0.0.1', [int]$Port = 41000)
. "$PSScriptRoot/environment.ps1"
$taskLibrary = (Join-Path $taskRoot 'target/release/hl100_core.dll').Replace('\','\\')
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class HL100Smoke {
 [DllImport("$taskLibrary", CallingConvention=CallingConvention.Cdecl)] private static extern IntPtr hl100_command([MarshalAs(UnmanagedType.LPUTF8Str)]string command);
 [DllImport("$taskLibrary", CallingConvention=CallingConvention.Cdecl)] private static extern void hl100_free(IntPtr value);
 public static string Call(string command) { var p=hl100_command(command);try{return Marshal.PtrToStringUTF8(p);}finally{hl100_free(p);} }
}
"@
$taskQa = Join-Path $taskRoot 'artifacts/qa'
New-Item -ItemType Directory -Force -Path $taskQa | Out-Null
$taskSource = Join-Path $taskQa 'ffi-проверка.txt'
Set-Content -LiteralPath $taskSource -Value 'HL100 Drop: UTF-8 JSON / native DLL / TCP reception'
$taskStart = @{op='start';config=@{id='ffi-smoke';name='FFI smoke';folder=$taskQa;port=41003}} | ConvertTo-Json -Compress
$taskState = [HL100Smoke]::Call($taskStart) | ConvertFrom-Json
if (!$taskState.ok) { throw $taskState.error }
try {
    $taskCommand = @{op='send';address=$Address;port=$Port;name='QA receiver';paths=@($taskSource)} | ConvertTo-Json -Compress
    $taskState = [HL100Smoke]::Call($taskCommand) | ConvertFrom-Json
    if (!$taskState.ok) { throw $taskState.error }
    $taskDeadline = (Get-Date).AddSeconds(25)
    do {
        Start-Sleep -Milliseconds 200
        $taskPoll = [HL100Smoke]::Call('{"op":"poll"}') | ConvertFrom-Json
        $taskDone = $taskPoll.events | Where-Object { $_.type -eq 'complete' } | Select-Object -First 1
        if ($taskDone) { if ($taskDone.status -ne 'done') { throw $taskDone.message }; Write-Output 'PASS: native FFI -> TCP -> receiver'; break }
    } while ((Get-Date) -lt $taskDeadline)
    if (!$taskDone) { throw 'FFI transfer timed out' }
} finally { [HL100Smoke]::Call('{"op":"stop"}') | Out-Null }
