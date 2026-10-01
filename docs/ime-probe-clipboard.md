# Probe: ClipSync Keyboard (IME) × ClipboardManager — Android 17 / API 37

Data: 2026-09-19 · Emulador Pixel 8a (`sdk_gphone16k_x86_64`, Android 17, API 37, imagem Play Store) · targetSdk 36.
Código: `ime/ClipboardProbe.kt`, ligado em `ClipSyncImeService` **só em builds debug** (`BuildConfig.DEBUG`).
Log: `adb logcat -s ClipSyncImeProbe` — só hash (SHA-256, 4 bytes), tamanho, mimetype, flags. Texto nunca sai do processo, nada é persistido, nada vai para o núcleo Rust nem para a rede.

Pergunta: com o ClipSync Keyboard **selecionado** como teclado padrão, o processo do app recebe `OnPrimaryClipChangedListener`
e lê `getPrimaryClip()` sem ter foco de janela? (Complementa `probes/fgs-clipboard` — categoria B: FGS **não** consegue.)

## Resultado: categoria **A** — IME padrão lê o clipboard em segundo plano

| # | Cenário | Activity do ClipSync | IME visível | Listener | `getPrimaryClip()` | Evidência |
|---|---|---|---|---|---|---|
| 1 | `ime set` (seleção) — sem nenhum campo focado | nunca aberta neste boot | não | — | **lido** (`len=8`, clipboard do host) | `read reason=start hasPrimaryClip=true clip=true` |
| 2 | Campo do Contatos: `input text` + Ctrl+A/Ctrl+C | não | sim | **dispara (3×)** | lido `len=19` | `listener#1..3 hash=c272c7bd dup=false/true/true` |
| 3 | HOME → Configurações → "Sobre o dispositivo" → long-press *Build number* → Copy | não | **não** | **dispara (3×)** | lido `len=24` | `listener#4 hash=52a8b20e imeShown=false took=6ms` |
| 4 | Tela bloqueada/desligada (poll de 5 s) | não | não | — | `hasPrimaryClip=true`, descrição visível | `poll#33..34 hasPrimaryClip=true` |
| 5 | Desbloqueio → copiar IMEI | não | não | dispara (3×) | lido `len=15` | `listener#9 hash=08a254c3` |
| 6 | `am force-stop app.clipsync.desktop` | — | — | serviço morto | — | **Android troca o teclado padrão de volta para o Gboard** (`default_input_method` = LatinIME); IME continua *habilitado*, mas não selecionado. Re-selecionar restaura. |
| 7 | `adb reboot` (com ClipSync Keyboard selecionado) | nunca aberta | não | serviço **criado no boot** (`onCreate`, `pid` novo, listener registrado) | clipboard vazio pós-boot (`hasPrimaryClip=false`) | `lifecycle onCreate` sem nenhum campo focado |
| 8 | Pós-reboot, desbloquear, copiar *Build number* | nunca aberta | não | **dispara** | lido `len=24` (`hash=52a8b20e`, igual ao #3) | `listener#1 took=95ms` |
| 9 | IME habilitado mas **não selecionado** (Gboard padrão) → copiar IMEI | — | — | serviço destruído (`onDestroy`, `stop: listener removed`) | **nada** | zero linhas de log |

Observações adicionais:

- **procState do processo com o IME selecionado = 7 (`IMPORTANT_BACKGROUND`, `IMPB` no `dumpsys`)** com a Activity fechada e teclado oculto. Não é *cached* → no Android 15+ a rede do processo **não** é cortada nesse estado (a cortar só acontece em *cached*). Relevante para manter o P2P vivo na etapa seguinte — a confirmar com socket real.
- **Sem aviso "ClipSync colou da área de transferência"** (Android 12+) nas leituras feitas pelo IME (screenshot 1,5 s após a cópia; `clipboard_show_access_notifications` no padrão). Leitura pelo IME padrão é isenta.
- **O listener dispara 2–3 vezes por cópia** (mesmo hash). A ponte real precisa deduplicar por hash antes de chamar o núcleo (o `handle_text` do Rust já faz isso via `last_seen_hash`, mas evitar a chamada JNI repetida é melhor).
- Leitura leva 1–6 ms (95 ms na primeira após o boot).
- `EXTRA_IS_SENSITIVE` não foi exercitado (nenhum app do emulador marca); o código já lê a flag e a repassará como `excluded_by_source`.
- Emulador compartilha clipboard com o host: o `len=8` do cenário 1 é o clipboard do Windows. Os cenários 2–8 usam textos com tamanho conhecido para isolar a direção.
- Ambiente: partição `/data` do emulador está com ~390 MB livres; para instalar foi preciso baixar o limiar de "pouco espaço" (`settings put global sys_storage_threshold_percentage 1`, `sys_storage_threshold_max_bytes 52428800`). Alternativa é recriar o AVD com `disk.dataPartition.size` maior (o emulador não redimensiona imagem existente).

## Consequências para a Etapa 3b (ponte real)

1. Caminho confirmado: `OnPrimaryClipChangedListener` no `InputMethodService` → hash/dedupe → `nativeOnClip(text, sensitive)` → `clipboard::handle_text`.
2. Núcleo Rust só existe com a Activity criada (Tauri). Cenários 3, 7 e 8 mostram que o IME roda **sem** Activity: precisa do *inbox* selado (KeyGuard) para quando o core não está vivo, drenado no próximo `setup`.
3. Force-stop desfaz a seleção do teclado: documentar na UI ("⚠ instalado, mas não selecionado") — o app já detecta esse estado ao voltar ao foreground.
4. Reboot mantém a seleção e sobe o serviço sozinho: o IME é o ponto de vida mais confiável do processo; se um dia o core for desacoplado do Tauri, é aqui que ele poderia nascer.
5. Remover `ClipboardProbe` (ou mantê-lo desligado) quando a ponte real entrar — não deixar dois leitores.
