# Ferramenta de teste (3c.2): envia um arquivo de um perfil para outro via CDP,
# espera terminar e confere o SHA-256 do destino. Não faz parte do produto.
# Uso: .\scripts\ft.ps1 -From a -To b -Path E:\clipsync-build\ft\10mb.bin [-TimeoutSec 600]
param(
  [string]$From = "a",
  [string]$To = "b",
  [Parameter(Mandatory = $true)][string]$Path,
  [int]$TimeoutSec = 600,
  [switch]$NoWait
)
Set-Location $PSScriptRoot\..
$toId = (node scripts/cdp.mjs $To "invoke('bootstrap').then(b=>b.me.deviceId)").Trim('"')
$p = $Path -replace '\\', '\\\\'
$rec = node scripts/cdp.mjs $From "invoke('transfer_send',{deviceId:'$toId',path:'$p'})" | ConvertFrom-Json
if (-not $rec.id) { throw "transfer_send falhou: $rec" }
"id=$($rec.id) name=$($rec.safeName) size=$($rec.size) state=$($rec.state.kind)"
if ($NoWait) { return $rec.id }
$sw = [Diagnostics.Stopwatch]::StartNew()
while ($sw.Elapsed.TotalSeconds -lt $TimeoutSec) {
  Start-Sleep -Milliseconds 500
  $t = node scripts/cdp.mjs $From "invoke('transfer_list',{limit:200}).then(l=>l.find(x=>x.id==='$($rec.id)'))" | ConvertFrom-Json
  if ($t.state.kind -in @("completed", "failed", "cancelled")) { break }
}
$r = node scripts/cdp.mjs $To "invoke('transfer_list',{limit:200}).then(l=>l.find(x=>x.id==='$($rec.id)'))" | ConvertFrom-Json
"sender: state=$($t.state.kind) reason=$($t.state.error)$($t.state.reason) bytes=$($t.bytesDone)/$($t.size) took=$([int]$sw.Elapsed.TotalSeconds)s"
"receiver: state=$($r.state.kind) final=$($r.finalPath) sha_expected=$($r.sha256Expected.Substring(0,[Math]::Min(16,$r.sha256Expected.Length))) sha_actual=$(if($r.sha256Actual){$r.sha256Actual.Substring(0,16)})"
if ($r.finalPath -and (Test-Path -LiteralPath $r.finalPath)) {
  $src = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLower()
  $dst = (Get-FileHash -LiteralPath $r.finalPath -Algorithm SHA256).Hash.ToLower()
  "sha256 source=$($src.Substring(0,16)) dest=$($dst.Substring(0,16)) match=$($src -eq $dst) size_ok=$((Get-Item -LiteralPath $r.finalPath).Length -eq (Get-Item -LiteralPath $Path).Length)"
}
