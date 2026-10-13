param([string]$Version = '0.1.0', [switch]$SkipInstaller)
. "$PSScriptRoot/environment.ps1"
Push-Location $taskRoot
try {
    cargo build --release --locked
    Assert-Exit 'Rust build'
    New-Item -ItemType Directory -Force -Path target/release | Out-Null
    [IO.File]::WriteAllBytes((Join-Path $taskRoot 'target/release/hl100_core.dll'), [IO.File]::ReadAllBytes((Join-Path $env:CARGO_TARGET_DIR 'release/hl100_core.dll')))
    dotnet publish apps/windows/HL100.Drop.csproj -c Release -r win-x64 --self-contained true -p:Version=$Version -o artifacts/windows
    Assert-Exit '.NET publish'
    if (!(Test-Path artifacts/windows/hl100_core.dll)) { throw 'Rust DLL was not included' }
    if (!$SkipInstaller) {
        $taskIscc = Join-Path $taskRoot '.tools/inno/ISCC.exe'
        if (!(Test-Path $taskIscc)) { $taskIscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe' }
        if (!(Test-Path $taskIscc)) { $taskIscc = (Get-Command ISCC.exe -ErrorAction Stop).Source }
        & $taskIscc /Q "/DAppVersion=$Version" "/DPublishDir=$(Join-Path $taskRoot 'artifacts/windows')" installer/windows/setup.iss
        Assert-Exit 'Installer build'
    }
} finally { Pop-Location }
