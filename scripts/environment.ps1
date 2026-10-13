$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path $PSScriptRoot -Parent
if (!("HL100ShortPath" -as [type])) {
    Add-Type -TypeDefinition 'using System;using System.Text;using System.Runtime.InteropServices;public static class HL100ShortPath{[DllImport("kernel32.dll",CharSet=CharSet.Unicode)]public static extern uint GetShortPathName(string path,StringBuilder b,uint size);}'
}
$taskBuffer = [System.Text.StringBuilder]::new(4096)
$taskLength = [HL100ShortPath]::GetShortPathName($taskRoot,$taskBuffer,4096)
$taskBuildRoot = if ($taskLength -gt 0) { $taskBuffer.ToString() } else { $taskRoot }
$env:CARGO_TARGET_DIR = Join-Path $taskBuildRoot 'target'
if (Test-Path (Join-Path $taskRoot '.tools/cargo/bin/cargo.exe')) {
    $env:CARGO_HOME = Join-Path $taskBuildRoot '.tools/cargo'
    $env:RUSTUP_HOME = Join-Path $taskBuildRoot '.tools/rustup'
    if (Test-Path (Join-Path $taskRoot '.tools/rustup-path.txt')) { $env:RUSTUP_HOME = (Get-Content -Raw (Join-Path $taskRoot '.tools/rustup-path.txt')).Trim() }
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
if (Test-Path (Join-Path $taskRoot '.tools/gcc/mingw64/bin')) {
    $env:PATH = "$(Join-Path $taskRoot '.tools/gcc/mingw64/bin');$env:PATH"
}
$taskAsciiGcc = Join-Path $env:TEMP 'hl100-drop-tools/mingw64/bin'
if (Test-Path (Join-Path $taskRoot '.tools/gcc-path.txt')) { $taskAsciiGcc = (Get-Content -Raw (Join-Path $taskRoot '.tools/gcc-path.txt')).Trim() }
if (Test-Path $taskAsciiGcc) { $env:PATH = "$taskAsciiGcc;$env:PATH" }
if (Test-Path (Join-Path $taskRoot '.tools/target-path.txt')) { $env:CARGO_TARGET_DIR = (Get-Content -Raw (Join-Path $taskRoot '.tools/target-path.txt')).Trim() }
function Assert-Exit([string]$taskName) { if ($LASTEXITCODE -ne 0) { throw "$taskName failed (exit $LASTEXITCODE)" } }
