#!/bin/bash
# Helpers de lifecycle (3c.4). Uso: source scripts/lc.sh
export PATH="/c/Users/wwall/AppData/Local/Android/Sdk/platform-tools:$PATH"
cd /c/projetos-pessoal/ClipSync
pid() { adb shell pidof app.clipsync.desktop | tr -d '\r'; }
fgs() { adb shell "dumpsys activity services app.clipsync.desktop 2>/dev/null | grep -c 'isForeground=true'" | tr -d '\r'; }
procstate() { adb shell "dumpsys activity processes | grep -A2 'ProcessRecord.*app.clipsync.desktop' | grep -o 'procState=[A-Z_]*' | head -1" | tr -d '\r'; }
launch() { adb shell input keyevent KEYCODE_HOME; sleep 1; adb shell input swipe 540 1800 540 600 300; sleep 2; adb shell input tap 909 850; sleep "${1:-8}"; }
cdp() { scripts/android-cdp.sh >/dev/null; }
and_id() { cdp; node scripts/cdp.mjs android "invoke('bootstrap').then(b=>b.me.deviceId)" | tr -d '"'; }
pc_id() { node scripts/cdp.mjs a "invoke('bootstrap').then(b=>b.me.deviceId)" | tr -d '"'; }
send_wa() { node scripts/cdp.mjs a "invoke('transfer_send',{deviceId:'$(and_id)',path:'$1'}).then(t=>t.id)" | tr -d '"'; }
send_aw() { cdp; node scripts/cdp.mjs android "invoke('transfer_send',{deviceId:'$(pc_id)',path:'$1'}).then(t=>t.id)" | tr -d '"'; }
st_pc() { node scripts/cdp.mjs a "invoke('transfer_list',{limit:20}).then(l=>{const t=l.find(x=>x.id==='$1');return t?[t.state.kind,t.state.error||t.state.reason||'',t.bytesDone,t.finalPath||'']:null})"; }
st_and() { cdp; node scripts/cdp.mjs android "invoke('transfer_list',{limit:20}).then(l=>{const t=l.find(x=>x.id==='$1');return t?[t.state.kind,t.state.error||t.state.reason||'',t.bytesDone,t.finalPath||'']:null})"; }
wait_pc() { for i in $(seq 1 $((${2:-300}*2))); do sleep 0.5; S=$(st_pc "$1" | grep -o '^\["[a-z]*' | tr -d '["'); case "$S" in completed|failed|cancelled) break;; esac; done; st_pc "$1"; }
wait_and() { for i in $(seq 1 $((${2:-300}*2))); do sleep 0.5; S=$(st_and "$1" | grep -o '^\["[a-z]*' | tr -d '["'); case "$S" in completed|failed|cancelled) break;; esac; done; st_and "$1"; }
sha_and() { adb shell "sha256sum '/sdcard/Download/ClipSync/$1'" | cut -c1-16; }
sha_pc() { sha256sum "$(cygpath -u "$1")" | cut -c1-16; }
parts() { adb shell "run-as app.clipsync.desktop ls -la transfers/.part/ transfers/" | tr -d "\r"; }
logf() { adb logcat -d -s RustStdoutStderr:* ClipSyncTransfer:* | grep -i "FILE\]\|ClipSyncTransfer" | grep -v "control message\|data frame" | tail -${1:-15}; }
wait_conn() { for i in $(seq 1 90); do S=$(node scripts/cdp.mjs a "invoke('bootstrap').then(b=>b.devices.find(d=>d.deviceId==='a2462c7ac1e64725557c8e9e89130857').state)" | tr -d '"'); [ "$S" = connected ] && { echo "conn=$S t=$i"; return; }; sleep 1; done; echo "NOT CONNECTED"; }
