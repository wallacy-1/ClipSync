# Sobe um perfil sem herdar CLIPSYNC_PROFILE de outra chamada na mesma sessão
# (run.ps1 usa a variável para achar/matar a instância anterior do mesmo perfil).
# Uso: .\scripts\start-profile.ps1 a 9222
param([string]$Profile = "a", [int]$CdpPort = 0)
$p = Start-Process powershell -ArgumentList "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "Set-Location '$PSScriptRoot\..'; .\run.ps1 $Profile $CdpPort" -PassThru -WindowStyle Hidden
$p.WaitForExit()
