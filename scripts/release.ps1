# Gera os pacotes de release do ClipSync.
#
#   .\scripts\release.ps1 windows            instalador Windows assinado p/ updater + latest.json
#   .\scripts\release.ps1 android            APK Android assinado (arm64 por padrão)
#   .\scripts\release.ps1 all                os dois
#   .\scripts\release.ps1 android-key        cria o keystore de release do Android (uma vez só)
#
# Chaves e senhas NUNCA ficam no repositório: são lidas de -KeysDir
# (padrão: $env:CLIPSYNC_KEYS_DIR, senão ..\chaves\ClipSync ao lado do repo).
# Saída: release\v<versão>\ (ignorada pelo git). Guia: docs/release.md
param(
    [Parameter(Position = 0)][ValidateSet("windows", "android", "all", "android-key")][string]$Target = "all",
    [string]$KeysDir = "",
    [string]$Repo = "",
    [string]$Notes = "",
    [string[]]$AndroidTargets = @("aarch64"),
    # Sem keystore de release: assina com ~/.android/debug.keystore (só para testes, nunca publicar).
    [switch]$DebugSign
)
$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root

function Fail($msg) { Write-Host "ERRO: $msg" -ForegroundColor Red; exit 1 }
function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }

function Invoke-Checked([string]$exe, [string[]]$argv) {
    & $exe @argv
    if ($LASTEXITCODE -ne 0) { Fail "$exe saiu com código $LASTEXITCODE" }
}

$conf = Get-Content (Join-Path $Root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$Version = $conf.version
$Out = Join-Path $Root "release\v$Version"
# $PSScriptRoot não existe nos valores padrão do param() no Windows PowerShell 5.1.
if (-not $KeysDir) { $KeysDir = if ($env:CLIPSYNC_KEYS_DIR) { $env:CLIPSYNC_KEYS_DIR } else { Join-Path $Root "..\chaves\ClipSync" } }
$KeysDir = [IO.Path]::GetFullPath($KeysDir)

# Build do Rust fora de C: quando a máquina tem E:\clipsync-build (ver README).
if (-not $env:CARGO_TARGET_DIR -and (Test-Path "E:\clipsync-build")) {
    $env:CARGO_TARGET_DIR = "E:\clipsync-build\target"
    $env:TMP = "E:\clipsync-build\tmp"; $env:TEMP = $env:TMP
    $env:GRADLE_USER_HOME = "E:\clipsync-build\gradle"
}
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root "src-tauri\target" }

function Read-Secret([string]$file) {
    $p = Join-Path $KeysDir $file
    if (-not (Test-Path $p)) { Fail "não achei $p (pasta de chaves: $KeysDir)" }
    return (Get-Content $p -Raw).Trim()
}

function Set-UpdaterSigning {
    $env:TAURI_SIGNING_PRIVATE_KEY = Read-Secret "updater.key"
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Read-Secret "updater.password"
}

function Get-RepoSlug {
    if ($Repo) { return $Repo }
    if ((git remote) -contains "origin") {
        $url = git remote get-url origin
        if ($url -match "github\.com[:/](.+?)(\.git)?$") { return $Matches[1] }
    }
    # Sem remote ainda: o mesmo repo do endpoint em tauri.conf.json.
    $endpoint = $conf.plugins.updater.endpoints[0]
    if ($endpoint -match "github\.com/([^/]+/[^/]+)/") { return $Matches[1] }
    Fail "informe -Repo usuario/repositorio"
}

function Build-Windows {
    Step "Windows $Version"
    Set-UpdaterSigning
    Invoke-Checked "npm" @("run", "tauri", "build")
    $bundle = Join-Path $TargetDir "release\bundle"
    $setup = Get-ChildItem (Join-Path $bundle "nsis") -Filter "*_$Version*-setup.exe" | Select-Object -First 1
    if (-not $setup) { Fail "instalador NSIS não encontrado em $bundle\nsis" }
    $sig = "$($setup.FullName).sig"
    if (-not (Test-Path $sig)) { Fail "assinatura $sig não foi gerada (createUpdaterArtifacts / chave)" }

    New-Item -ItemType Directory -Force $Out | Out-Null
    Copy-Item $setup.FullName, $sig $Out -Force
    Get-ChildItem (Join-Path $bundle "msi") -Filter "*_$Version*.msi" -ErrorAction SilentlyContinue | Copy-Item -Destination $Out -Force

    # GitHub troca espaços por pontos no nome do asset.
    $asset = $setup.Name -replace " ", "."
    $slug = Get-RepoSlug
    $latest = [ordered]@{
        version   = $Version
        notes     = $(if ($Notes) { $Notes } else { "ClipSync $Version" })
        pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
        platforms = [ordered]@{
            "windows-x86_64" = [ordered]@{
                signature = (Get-Content $sig -Raw).Trim()
                url       = "https://github.com/$slug/releases/download/v$Version/$asset"
            }
        }
    }
    $json = $latest | ConvertTo-Json -Depth 5
    [IO.File]::WriteAllText((Join-Path $Out "latest.json"), $json, (New-Object Text.UTF8Encoding $false))
    Write-Host "OK: $Out\$($setup.Name) + latest.json" -ForegroundColor Green
}

function Find-Java {
    if ($env:JAVA_HOME -and (Test-Path "$env:JAVA_HOME\bin\java.exe")) { return }
    $studio = "C:\Program Files\Android\Android Studio\jbr"
    if (Test-Path $studio) { $env:JAVA_HOME = $studio; return }
    $jdk = Get-ChildItem "C:\Program Files\Java" -Directory -Filter "jdk*" -ErrorAction SilentlyContinue | Sort-Object Name | Select-Object -Last 1
    if (-not $jdk) { Fail "JDK não encontrado; defina JAVA_HOME" }
    $env:JAVA_HOME = $jdk.FullName
}

function Find-AndroidSdk {
    if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = Join-Path $env:LOCALAPPDATA "Android\Sdk" }
    if (-not (Test-Path $env:ANDROID_HOME)) { Fail "Android SDK não encontrado; defina ANDROID_HOME" }
    if (-not $env:NDK_HOME) {
        $ndk = Get-ChildItem (Join-Path $env:ANDROID_HOME "ndk") -Directory -ErrorAction SilentlyContinue | Sort-Object { [version]$_.Name } | Select-Object -Last 1
        if (-not $ndk) { Fail "NDK não encontrado; defina NDK_HOME" }
        $env:NDK_HOME = $ndk.FullName
    }
    $bt = Get-ChildItem (Join-Path $env:ANDROID_HOME "build-tools") -Directory | Sort-Object { [version]($_.Name -replace "-.*", "") } | Select-Object -Last 1
    if (-not $bt) { Fail "build-tools não encontrado no SDK" }
    return $bt.FullName
}

function New-AndroidKey {
    Find-Java
    $ks = Join-Path $KeysDir "android-release.jks"
    if (Test-Path $ks) { Fail "$ks já existe; não sobrescrevo (perder a chave impede atualizar o app instalado)" }
    New-Item -ItemType Directory -Force $KeysDir | Out-Null
    $chars = [char[]]"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789-_."
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    $bytes = New-Object byte[] 40; $rng.GetBytes($bytes)
    $pw = -join ($bytes | ForEach-Object { $chars[$_ % $chars.Length] })
    Invoke-Checked "$env:JAVA_HOME\bin\keytool.exe" @(
        "-genkeypair", "-keystore", $ks, "-alias", "clipsync", "-keyalg", "RSA", "-keysize", "4096", "-validity", "10000",
        "-storepass", $pw, "-keypass", $pw, "-dname", "CN=ClipSync")
    [IO.File]::WriteAllText((Join-Path $KeysDir "android.password"), $pw)
    Add-Content (Join-Path $KeysDir "SENHAS.md") "`n## Android (assinatura do APK)`n`n| Item | Valor |`n|---|---|`n| Keystore | ``$ks`` |`n| Alias | ``clipsync`` |`n| Senha (keystore e chave) | ``$pw`` |`n| Gerada em | $(Get-Date -Format yyyy-MM-dd) |`n" -Encoding UTF8
    Write-Host "OK: $ks (senha em SENHAS.md). Faça backup: sem ela o app publicado não pode ser atualizado." -ForegroundColor Green
}

function Build-Android {
    Step "Android $Version ($($AndroidTargets -join ', '))"
    Find-Java
    $buildTools = Find-AndroidSdk
    Set-UpdaterSigning

    $ks = Join-Path $KeysDir "android-release.jks"
    if (Test-Path $ks) {
        $ksPass = Read-Secret "android.password"; $alias = "clipsync"
    } elseif ($DebugSign) {
        Write-Host "AVISO: assinando com o keystore de DEBUG. Não publique este APK." -ForegroundColor Yellow
        $ks = Join-Path $env:USERPROFILE ".android\debug.keystore"; $ksPass = "android"; $alias = "androiddebugkey"
        if (-not (Test-Path $ks)) { Fail "$ks não existe" }
    } else {
        Fail "sem keystore de release em $KeysDir. Rode '.\scripts\release.ps1 android-key' (uma vez) ou use -DebugSign para teste"
    }

    # Daemon do Gradle herda o stdout e prende quem chamou o script com pipe.
    $env:GRADLE_OPTS = "-Dorg.gradle.daemon=false"
    $argv = @("tauri", "android", "build", "--apk")
    foreach ($t in $AndroidTargets) { $argv += @("--target", $t) }
    Invoke-Checked "npx" $argv

    $unsigned = Get-ChildItem (Join-Path $Root "src-tauri\gen\android\app\build\outputs\apk") -Recurse -Filter "*-release-unsigned.apk" |
        Sort-Object LastWriteTime | Select-Object -Last 1
    if (-not $unsigned) { Fail "APK release não encontrado" }

    New-Item -ItemType Directory -Force $Out | Out-Null
    $aligned = Join-Path $Out "aligned.tmp.apk"
    $final = Join-Path $Out "ClipSync_$Version.apk"
    Remove-Item $aligned, $final -ErrorAction SilentlyContinue
    Invoke-Checked (Join-Path $buildTools "zipalign.exe") @("-f", "-p", "4", $unsigned.FullName, $aligned)
    Invoke-Checked (Join-Path $buildTools "apksigner.bat") @(
        "sign", "--v4-signing-enabled", "false", "--ks", $ks, "--ks-key-alias", $alias, "--ks-pass", "pass:$ksPass", "--key-pass", "pass:$ksPass", "--out", $final, $aligned)
    Remove-Item $aligned
    Invoke-Checked (Join-Path $buildTools "apksigner.bat") @("verify", $final)
    Write-Host "OK: $final" -ForegroundColor Green
}

switch ($Target) {
    "windows" { Build-Windows }
    "android" { Build-Android }
    "all" { Build-Windows; Build-Android }
    "android-key" { New-AndroidKey }
}
if ($Target -ne "android-key") {
    Write-Host "`nArquivos em $Out" -ForegroundColor Green
    Get-ChildItem $Out | ForEach-Object { "  $($_.Name)  ($([math]::Round($_.Length / 1MB, 1)) MB)" }
}
