# PC (perfil a) gera QR; Android (emulador, CDP 9225) pareia com o payload real do QR.
# Testa segredo errado primeiro (payload com secret alterado) e depois o correto.
Set-Location $PSScriptRoot\..
$info = node scripts/cdp.mjs a "invoke('start_pairing')" | ConvertFrom-Json
"PC pairing code=$($info.code)"
$payload = $info.payload | ConvertFrom-Json
"QR payload addrs=$($payload.addrs -join ',') port=$($payload.port)"

$wrong = $payload | Select-Object *
$wrong.secret = "0" * 64
$b64w = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes(($wrong | ConvertTo-Json -Compress)))
"--- código ERRADO:"
node scripts/cdp.mjs android "invoke('pair_with_payload',{raw:atob('$b64w')}).catch(e=>'rejeitado: '+e)"

$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($info.payload))
"--- código CERTO:"
node scripts/cdp.mjs android "invoke('pair_with_payload',{raw:atob('$b64')})"
Start-Sleep 3
"--- PC vê:"; node scripts/cdp.mjs a "invoke('list_devices').then(d=>d.map(x=>[x.name,x.platform,x.state,x.address]))"
"--- Android vê:"; node scripts/cdp.mjs android "invoke('list_devices').then(d=>d.map(x=>[x.name,x.platform,x.state,x.address]))"
