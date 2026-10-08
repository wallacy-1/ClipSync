# Teste 3/4: B cai, A copia (vai para a fila), B volta, fila é entregue.
Set-Location $PSScriptRoot\..
$procs = Get-CimInstance Win32_Process -Filter "Name = 'clipsync.exe'"
# A é a instância com CDP na porta 9222.
$pidA = (Get-NetTCPConnection -LocalPort 9222 -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1).OwningProcess
# 9222 pertence ao msedgewebview2 filho de A; subo até o clipsync pai
$pidAclip = $pidA
while ($pidAclip -and (Get-CimInstance Win32_Process -Filter "ProcessId = $pidAclip").Name -ne 'clipsync.exe') {
  $pidAclip = (Get-CimInstance Win32_Process -Filter "ProcessId = $pidAclip").ParentProcessId
}
$pidB = ($procs | Where-Object { $_.ProcessId -ne $pidAclip } | Select-Object -First 1).ProcessId
"A=$pidAclip B=$pidB -> matando B"
Stop-Process -Id $pidB -Force; Start-Sleep 4
node scripts/cdp.mjs a "invoke('list_devices').then(d=>d.map(x=>[x.name,x.state]))"
$tag = Get-Random
Set-Clipboard -Value "offline-$tag-1"; Start-Sleep 1; Set-Clipboard -Value "offline-$tag-2"; Start-Sleep 2
node scripts/cdp.mjs a "invoke('sync_status')"
"--- A log:"; Get-Content E:\clipsync-build\a.log | Select-String "queued|DEVICE\]" | Select-Object -Last 3 | ForEach-Object { $_.Line }
"--- religando B"; .\run.ps1 b 9223 | Out-Null; Start-Sleep 8
node scripts/cdp.mjs a "invoke('sync_status')"
Get-Content E:\clipsync-build\a.log | Select-String "Flushing|acknowledged|Connected" | Select-Object -Last 4 | ForEach-Object { $_.Line }
python scripts/dbcount.py offline-$tag
