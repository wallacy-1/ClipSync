# Probe: Foreground Service × ClipboardManager (Android 17 / API 37)

App isolado (não faz parte do ClipSync) para responder empiricamente: um Foreground Service
legítimo consegue observar o clipboard com a Activity em background?

Build/instalar: `./gradlew assembleDebug` · `adb install -r app/build/outputs/apk/debug/app-debug.apk` ·
`adb shell pm grant app.clipsync.probe android.permission.POST_NOTIFICATIONS` ·
`adb shell am start -n app.clipsync.probe/.MainActivity --ez start true` · logs: `adb logcat -s ClipProbe`.

Configuração usada: `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_DATA_SYNC` + `POST_NOTIFICATIONS`,
`<service android:foregroundServiceType="dataSync">`, `startForeground(id, notif, FOREGROUND_SERVICE_TYPE_DATA_SYNC)`,
`OnPrimaryClipChangedListener` + polling de 2 s (`hasPrimaryClip`, `primaryClipDescription`, `primaryClip`).
Logs só com evento + hash (nunca o texto).

## Resultado (emulador Pixel 8a, Android 17, targetSdk 36) — categoria **B**

| Cenário | Processo | `hasPrimaryClip` | `primaryClip` | Listener |
|---|---|---|---|---|
| Activity em foreground | FGS | true | lido (hash muda) | — |
| Activity em background (HOME + outro app copia) | FGS, `procState=FGS` (não cached) | **false** | **null** | **nunca dispara** |
| Tela bloqueada | FGS | false | null | — |
| Após 200 s em background + nova cópia | FGS vivo, 107 polls | false | null | 0 disparos |
| Volta ao foreground | FGS | true | lido no 1º poll (≤ 2 s) | — |

O Foreground Service mantém o processo vivo e fora do estado *cached*, mas o Android oculta o clipboard
(inclusive `hasPrimaryClip`/`ClipDescription`) de qualquer app sem foco de janela que não seja o teclado padrão.
Sem SecurityException: a API simplesmente responde "vazio". Não há workaround via FGS.
