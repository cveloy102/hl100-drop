param([switch]$Release)
. "$PSScriptRoot/environment.ps1"
if ($Release -and !$env:HL100_KEYSTORE -and (Test-Path (Join-Path $taskRoot 'private/android-signing.json'))) {
    $taskSigning = Get-Content -Raw (Join-Path $taskRoot 'private/android-signing.json') | ConvertFrom-Json
    $env:HL100_KEYSTORE = $taskSigning.store
    $env:HL100_STORE_PASSWORD = $taskSigning.password
    $env:HL100_KEY_PASSWORD = $taskSigning.password
    $env:HL100_KEY_ALIAS = $taskSigning.alias
}
if (!$env:JAVA_HOME) { $env:JAVA_HOME = 'C:\Program Files\Android\Android Studio\jbr' }
if (!$env:ANDROID_HOME) { $env:ANDROID_HOME = Join-Path $env:LOCALAPPDATA 'Android/Sdk' }
if (!$env:ANDROID_NDK_HOME) { $env:ANDROID_NDK_HOME = Join-Path $taskBuildRoot '.tools/android-ndk-r27d' }
$taskLlvm = Join-Path $env:ANDROID_NDK_HOME 'toolchains/llvm/prebuilt/windows-x86_64/bin'
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = Join-Path $taskLlvm 'aarch64-linux-android31-clang.cmd'
$env:CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER = Join-Path $taskLlvm 'x86_64-linux-android31-clang.cmd'
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS = '-C link-arg=-Wl,-z,max-page-size=16384'
$env:CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS = '-C link-arg=-Wl,-z,max-page-size=16384'
Push-Location $taskRoot
try {
    foreach ($taskTarget in @(@('aarch64-linux-android','arm64-v8a'),@('x86_64-linux-android','x86_64'))) {
        cargo build --release --locked --target $taskTarget[0]
        Assert-Exit "Rust $($taskTarget[0])"
        $taskDest = "apps/android/app/src/main/jniLibs/$($taskTarget[1])"
        New-Item -ItemType Directory -Force -Path $taskDest | Out-Null
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$($taskTarget[0])/release/libhl100_core.so") -Destination $taskDest
    }
    Push-Location apps/android
    try {
        if ($Release) { if (!$env:HL100_KEYSTORE) { throw 'Set HL100_KEYSTORE and signing environment variables for a release APK' }; & .\gradlew.bat assembleRelease }
        else { & .\gradlew.bat assembleDebug }
        Assert-Exit 'Android build'
    } finally { Pop-Location }
} finally { Pop-Location }
