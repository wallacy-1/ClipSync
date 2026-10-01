#!/bin/bash
# Teste (3c.3): Windows(a) → Android. Envia, espera, confere SHA-256 no MediaStore do emulador.
# Uso: scripts/ft-android.sh <arquivo-windows> [timeout_s]
cd "$(dirname "$0")/.."
F="$1"; T="${2:-300}"
AND=$(node scripts/cdp.mjs android "invoke('bootstrap').then(b=>b.me.deviceId)" | tr -d '"')
P=$(cygpath -m "$F")
ID=$(node scripts/cdp.mjs a "invoke('transfer_send',{deviceId:'$AND',path:'$P'}).then(t=>t.id)" | tr -d '"')
[ -z "$ID" ] && { echo "transfer_send falhou"; exit 1; }
echo "id=$ID"
for i in $(seq 1 $((T*2))); do
  sleep 0.5
  S=$(node scripts/cdp.mjs a "invoke('transfer_list',{limit:50}).then(l=>{const t=l.find(x=>x.id==='$ID');return t?t.state.kind:'?'})" | tr -d '"')
  case "$S" in completed|failed|cancelled) break;; esac
done
R=$(node scripts/cdp.mjs android "invoke('transfer_list',{limit:50}).then(l=>{const t=l.find(x=>x.id==='$ID');return t?[t.state.kind,t.state.error||t.state.reason||'',t.finalPath,t.safeName,(t.sha256Actual||'').slice(0,16),t.bytesDone]:null})")
echo "sender=$S receiver=$R"
NAME=$(echo "$R" | node -e 'let d="";process.stdin.on("data",c=>d+=c).on("end",()=>{const a=JSON.parse(d);console.log(a?a[3]:"")})')
SRC=$(sha256sum "$F" | cut -c1-64)
DST=$(adb shell "sha256sum '/sdcard/Download/ClipSync/$NAME'" 2>/dev/null | cut -c1-64)
echo "sha src=${SRC:0:16} dst=${DST:0:16} match=$([ "$SRC" = "$DST" ] && echo true || echo false) name=$NAME"
