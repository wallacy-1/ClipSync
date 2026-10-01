#!/bin/bash
# Teste (3c.3): Android → Windows(a). Origem = caminho privado do app ou content:// URI.
# Uso: scripts/ft-from-android.sh <source-no-android> [timeout_s] [sha_esperado]
cd "$(dirname "$0")/.."
SRC="$1"; T="${2:-300}"; EXP="$3"
PC=$(node scripts/cdp.mjs a "invoke('bootstrap').then(b=>b.me.deviceId)" | tr -d '"')
ID=$(node scripts/cdp.mjs android "invoke('transfer_send',{deviceId:'$PC',path:'$SRC'}).then(t=>t.id).catch(e=>'ERR '+e)" | tr -d '"')
case "$ID" in ERR*) echo "$ID"; exit 1;; esac
echo "id=$ID"
for i in $(seq 1 $((T*2))); do
  sleep 0.5
  S=$(node scripts/cdp.mjs android "invoke('transfer_list',{limit:50}).then(l=>{const t=l.find(x=>x.id==='$ID');return t?t.state.kind:'?'})" | tr -d '"')
  case "$S" in completed|failed|cancelled) break;; esac
done
A=$(node scripts/cdp.mjs android "invoke('transfer_list',{limit:50}).then(l=>{const t=l.find(x=>x.id==='$ID');return t?[t.state.kind,t.state.error||t.state.reason||'',t.bytesDone,(t.sha256Expected||'').slice(0,16)]:null})")
R=$(node scripts/cdp.mjs a "invoke('transfer_list',{limit:50}).then(l=>{const t=l.find(x=>x.id==='$ID');return t?[t.state.kind,t.state.error||t.state.reason||'',t.finalPath,(t.sha256Actual||'').slice(0,16)]:null})")
echo "android=$A pc=$R"
FP=$(echo "$R" | node -e 'let d="";process.stdin.on("data",c=>d+=c).on("end",()=>{const a=JSON.parse(d);console.log(a&&a[2]?a[2]:"")})')
if [ -n "$FP" ]; then DST=$(sha256sum "$(cygpath -u "$FP")" | cut -c1-64); echo "sha dst=${DST:0:16} expected=${EXP:0:16} match=$([ -n "$EXP" ] && [ "${DST:0:16}" = "${EXP:0:16}" ] && echo true || echo unknown) final=$FP"; fi
