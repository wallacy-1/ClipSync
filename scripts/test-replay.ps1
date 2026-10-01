# Teste de replay: pareia B->A via relay em modo REPLAY, B envia um item 2x;
# o relay reinjeta o 1º Clip cifrado. Esperado: A recusa (nonce antigo), sem duplicar.
Set-Location $PSScriptRoot\..
& $PSScriptRoot\test-relay-pair.ps1 -Replay | Select-Object -Last 1
$idA = (node scripts/cdp.mjs a "invoke('bootstrap').then(b=>b.me.deviceId)").Trim('"')
$db = 'C:\Users\wwall\AppData\Roaming\app.clipsync.desktop\profiles\a\clipsync.db'
$count = { python -c "import sqlite3;print(sqlite3.connect(r'$db').execute('select count(*) from clipboard_items').fetchone()[0])" }
"A items before: $(& $count)"
$item = (node scripts/cdp.mjs b "invoke('latest_item').then(i=>i.id)").Trim('"')
"B envia item $item para A duas vezes"
node scripts/cdp.mjs b "invoke('send_item',{itemId:'$item', deviceIds:['$idA']})"
Start-Sleep 1
node scripts/cdp.mjs b "invoke('send_item',{itemId:'$item', deviceIds:['$idA']})"
Start-Sleep 4
"A items after: $(& $count)"
Select-String -Path E:\clipsync-build\relay-replay.log -Pattern "REPLAY" | ForEach-Object { $_.Line }
"--- A log:"; Get-Content E:\clipsync-build\a.log | Select-String "SYNC\]|DEVICE\]|decrypt|AUTH|closed" | Select-Object -Last 8 | ForEach-Object { $_.Line }
