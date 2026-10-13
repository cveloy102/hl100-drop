. "$PSScriptRoot/environment.ps1"
$taskPrivate = Join-Path $taskRoot 'private'
$taskCredentials = Join-Path $taskPrivate 'android-signing.json'
if (Test-Path $taskCredentials) { Write-Output 'Android signing key already exists'; exit 0 }
New-Item -ItemType Directory -Force -Path $taskPrivate | Out-Null
$taskPassword = [Convert]::ToBase64String([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
$taskStore = Join-Path $taskPrivate 'hl100-drop.jks'
$env:HL100_GENERATED_PASSWORD = $taskPassword
try {
    $taskJava = if ($env:JAVA_HOME) { $env:JAVA_HOME } else { 'C:\Program Files\Android\Android Studio\jbr' }
    & (Join-Path $taskJava 'bin/keytool.exe') -genkeypair -keystore $taskStore -storetype PKCS12 -storepass:env HL100_GENERATED_PASSWORD -keypass:env HL100_GENERATED_PASSWORD -alias hl100 -keyalg RSA -keysize 3072 -validity 36500 -dname 'CN=HL100 Drop, OU=HL100'
    Assert-Exit 'Android signing key'
    @{store=$taskStore;password=$taskPassword;alias='hl100'} | ConvertTo-Json | Set-Content -LiteralPath $taskCredentials
    Write-Output 'Android signing key created in private/. Keep this directory for future updates; it is excluded from Git.'
} finally { Remove-Item Env:\HL100_GENERATED_PASSWORD }
