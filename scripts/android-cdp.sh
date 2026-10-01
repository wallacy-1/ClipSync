#!/bin/bash
# Abre o app no emulador (se preciso) e liga a porta CDP 9225 à WebView do ClipSync.
# Uso: scripts/android-cdp.sh [--launch]
export PATH="/c/Users/wwall/AppData/Local/Android/Sdk/platform-tools:$PATH"
if [ "$1" = "--launch" ]; then
  adb shell am start -n app.clipsync.desktop/.MainActivity >/dev/null
  sleep 4
fi
PID=$(adb shell pidof app.clipsync.desktop | tr -d '\r')
[ -z "$PID" ] && { echo "app não está rodando"; exit 1; }
adb forward tcp:9225 localabstract:webview_devtools_remote_$PID >/dev/null
echo "pid=$PID cdp=9225"
