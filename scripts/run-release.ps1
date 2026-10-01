# Igual a run.ps1, mas com o binário RELEASE (medições de throughput).
# Uso: .\scripts\run-release.ps1 a 9222
param([string]$Profile = "a", [int]$CdpPort = 0)
Get-Process clipsync -ErrorAction SilentlyContinue | Where-Object { $_.StartInfo.EnvironmentVariables["CLIPSYNC_PROFILE"] -eq $Profile } | Stop-Process -Force -ErrorAction SilentlyContinue
$env:CLIPSYNC_PROFILE = $Profile
$env:RUST_LOG = "clipsync_lib=debug,clipsync=debug"
$env:WEBVIEW2_USER_DATA_FOLDER = "E:\clipsync-build\webview-$Profile"
if ($CdpPort -gt 0) { $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$CdpPort" } else { Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue }
$log = "E:\clipsync-build\$Profile.log"
Remove-Item $log -ErrorAction SilentlyContinue
$p = Start-Process -FilePath "E:\clipsync-build\target\release\clipsync.exe" -RedirectStandardError $log -PassThru
Start-Sleep 5
"profile=$Profile pid=$($p.Id) exited=$($p.HasExited) log=$log cdp=$CdpPort release=1"
