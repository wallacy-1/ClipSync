# Teste: despareia A e B, sobe um relay que grava bytes, e pareia B -> A através dele.
# Depois disso B reconecta sempre via relay (último endereço conhecido).
param([switch]$Replay)
Set-Location $PSScriptRoot\..
$idA = (node scripts/cdp.mjs a "invoke('bootstrap').then(b=>b.me.deviceId)").Trim('"')
$idB = (node scripts/cdp.mjs b "invoke('bootstrap').then(b=>b.me.deviceId)").Trim('"')
"idA=$idA idB=$idB"
node scripts/cdp.mjs a "invoke('remove_device',{deviceId:'$idB'}).then(d=>'A devices: '+d.length)"
node scripts/cdp.mjs b "invoke('remove_device',{deviceId:'$idA'}).then(d=>'B devices: '+d.length)"
Start-Sleep 2

$logName = if ($Replay) { "relay-replay" } else { "relay" }
Get-CimInstance Win32_Process -Filter "Name = 'node.exe'" | Where-Object { $_.CommandLine -like "*relay.mjs*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
Start-Sleep 1
if (Test-Path "E:\clipsync-build\$logName.log") { Remove-Item "E:\clipsync-build\$logName.log" }
if (Test-Path "E:\clipsync-build\$logName.log.raw") { Remove-Item "E:\clipsync-build\$logName.log.raw" }
if ($Replay) { $env:REPLAY = "1" } else { $env:REPLAY = "0" }
Start-Process node -ArgumentList "scripts/relay.mjs 47999 127.0.0.1 47820 E:\clipsync-build\$logName.log" -WindowStyle Hidden
Start-Sleep 1

$info = node scripts/cdp.mjs a "invoke('start_pairing')" | ConvertFrom-Json
$payload = $info.payload | ConvertFrom-Json
$payload.addrs = @("127.0.0.1"); $payload.port = 47999
$json = $payload | ConvertTo-Json -Compress
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
"payload via relay: $json"
node scripts/cdp.mjs b "invoke('pair_with_payload',{raw:atob('$b64')})"
Start-Sleep 3
node scripts/cdp.mjs b "invoke('list_devices').then(d=>d.map(x=>[x.name,x.state,x.address]))"
