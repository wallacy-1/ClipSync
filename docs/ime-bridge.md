# ClipSync Keyboard → núcleo → P2P (Etapa 3b)

Data: 2026-09-19 · Emulador Pixel 8a, Android 17 / API 37 · PC: perfil `a` (Windows, `run.ps1 a 9222`).
Resultado: **Android → Windows automático com a Activity do ClipSync fechada — PASSOU** (testes A–F abaixo).

## Arquitetura

```
ClipboardManager (Android)
  └─ OnPrimaryClipChangedListener            ime/ClipboardBridge.kt   (único leitor do IME)
       ├─ text/plain? blank? EXTRA_IS_SENSITIVE
       ├─ SHA-256 + dedupe (mesmo hash < 3 s = mesmo evento; listener dispara 2–5×)
       └─ worker thread
            ├─ CoreBridge.ready?  ──► CoreBridge.nativeOnClip(text, sensitive)   (JNI, ime/CoreBridge.kt)
            │                          └─ src-tauri/src/ime.rs ──► clipboard::handle_text(.., ClipOrigin::Ime)
            │                                                        ├─ DedupeState.decide()  (anti-loop + dup)
            │                                                        ├─ classify · SQLite · expiração · evento
            │                                                        └─ sync::on_local_clip ──► Noise/TCP ──► Windows
            └─ senão ─────────────► ImeInbox.put()  files/ime_inbox/<ts>-<id>.sealed  (KeyGuard AES-GCM)
                                       ▲ drenado por ime::drain_inbox() no setup do Tauri e em clipboard_poll_now
                                       └─ cada item: handle_text(.., ClipOrigin::ImeInbox) → removeInbox(id)
```

- **Kotlin fino**: extrai texto/flag/hash, deduplica, escolhe a rota. Nenhuma regra de negócio.
- **Rust dono do pipeline**: `handle_text` é o único ponto de entrada para watcher, poll, IME e inbox
  (`ClipOrigin` só entra no log). Nada foi adicionado ao protocolo P2P.
- **`CoreBridge.ready`** é ligado pelo Rust (`ime::attach`) no fim do `setup`; antes disso o Kotlin nem
  toca na biblioteca nativa (o processo pode ter nascido só para o IME).

## Dedupe / anti-loop unificados (`clipboard/dedupe.rs`, 6 testes)

| Memória | Quem grava | Quem consulta | Efeito |
|---|---|---|---|
| `last_written` | `write_local` (todo conteúdo remoto aplicado no clipboard — `sync.rs` agora passa por aqui) | `decide()` | eco do IME/watcher após escrita remota → `WrittenBySelf`, nunca reenvia |
| `last_handled (hash, ts)` | `decide()` | `decide()` | mesmo hash < 2 s por qualquer origem → `Duplicate` |
| `last_seen` | `decide()`, `note_written`, `prime` (clipboard no boot), `poll_changed` | `poll_changed` | watcher/`clipboard_poll_now` só reagem a hash novo; não repetem o que o IME já entregou |

Estratégia: **evento** (IME, inbox) → `decide()`; **polling** (watcher, volta ao foreground) → `poll_changed()` e só então `decide()`.

## Inbox (`ime/ImeInbox.kt`)

- Só é usado quando `nativeOnClip` não pode ser chamada (núcleo não vivo) ou devolve `false` (pânico).
- Arquivo por item em `files/ime_inbox/`, conteúdo = `KeyGuard.seal(JSON{id, hash, text, ts, sensitive})` —
  mesma chave AES-256-GCM não exportável do Keystore que protege a KEK. Nenhuma chave nova, nada em claro
  (verificado: `grep` do texto nos `.sealed` = 0).
- TTL 1 h (`ImeInbox.TTL_MS` = `ime::INBOX_TTL_MS`; espelha a limpeza do clipboard do Android 13+). Máx. 50 itens.
  Mesmo hash no fim da fila não duplica. Escrita atômica (`.tmp` → rename).
- Drain: Rust chama `CoreBridge.listInbox` (Kotlin decifra, descarta expirados/ilegíveis) → para cada item,
  `handle_text` → só então `CoreBridge.removeInbox(id)`. Pânico em `handle_text` mantém o item; item ignorado por
  dedupe/anti-loop conta como processado. Sem dispositivo conectado no momento, o item vai para o outbox e é
  entregue no `Flushing outbox` da conexão seguinte.

## Ciclo de vida observado

- **HOME / app em background**: processo vivo com o núcleo (`procState=7`) → rota `core`, envio imediato.
- **Back na Activity raiz**: o Tauri **encerra o processo** (`am_proc_died` 1 s depois); o sistema recria o processo só
  com o IME → rota `inbox` até o próximo `setup`. Mesmo comportamento do reboot.
- **Selecionado outro teclado**: serviço destruído, listener removido, nada é lido.
- **force-stop**: Android troca o teclado padrão de volta (documentado em 3a).

## Testes executados

Emulador; PC com `autoSend=false` (e `autoReceive=false` a partir do teste de tela bloqueada) para isolar a direção,
porque o emulador espelha o clipboard host↔guest nos dois sentidos.

| Teste | Passos | Resultado | Evidência (logs só com hash) |
|---|---|---|---|
| **A** IME → Rust local | Activity em background (HOME); copiar IMEI em Configurações | ✅ | `CLIPBOARD_IME event hash=08a254c3 len=15 route=core` → `[CLIPBOARD] Change detected origin=ime type=card sensitive=true` (IMEI classificado como cartão → sensível → não sincroniza por regra; item no histórico) |
| **B** Android → Windows | idem, copiar Bluetooth address (17 B) | ✅ | Android: `Change detected origin=ime hash=b597275f` → `Item sent` → `Item acknowledged`; PC: `Item received … from=813b682a bytes=17` → `apply_clipboard=true`; `latest_item` no PC = conteúdo correto, `sourceDeviceId` = Android; 1 linha em cada histórico |
| **C** anti-loop | PC `send_item` → Android | ✅ | Android: `Item received` → `Writing to system clipboard hash=fc83d41a` → IME dispara → `Change ignored (written by ClipSync) origin=ime`; **nenhum** `Item sent` do Android; 1 item no histórico Android. (PC mostrou `Change detected origin=poll` = clipboard do host espelhado pelo emulador, não P2P) |
| **D** inbox | núcleo morto (processo só do IME) → copiar → `route=inbox`, 1 arquivo `.sealed` (218 B), sem texto em claro → abrir ClipSync | ✅ | `[IME] Draining inbox count=1` → `Change detected origin=ime-inbox` → `Item queued` → `Connecting` → `Flushing outbox count=1` → `acknowledged`; PC `Item received`; inbox vazio após |
| **E** reboot | `adb reboot`, não abrir o app, copiar IMEI + Bluetooth | ✅ | pós-boot: `service created`, `listener registered`; cópias → `route=inbox` (2 arquivos); abrir app → `Draining inbox count=2` → Bluetooth `Item sent` / PC `Item received`; IMEI só no histórico (sensível) |
| **F** duplicação | cada cópia gerou 3–5 callbacks | ✅ | `event … route=core` + N× `event dup hash=…`; Rust vê 1 chamada; 1 item; 1 `Item sent`; PC 1 `Item received` |
| Tela bloqueada | HOME, tela desligada 20 s, desbloquear, copiar | ✅ | conexão sobreviveu; `Item sent` → PC `Item received` imediato |

Qualidade: `cargo test` (7 ok, 6 novos em `dedupe.rs`) · `cargo clippy -D warnings` · `tsc --noEmit` · Gradle/Kotlin sem erros · APK 49 MB.
Privacidade: `logcat --pid` do processo filtrado pelos textos copiados = 0 ocorrências.

## Limitações / observações

1. Emulador espelha o clipboard host↔guest nos dois sentidos: cada escrita do ClipSync no clipboard do Windows
   reaparece no Android como "cópia" e vice-versa. Gera itens extras `origin=poll` no PC e um reenvio tardio
   (>2 s) do mesmo hash pelo IME. Não acontece em aparelho real; testes isolaram com `autoSend/autoReceive=false` no PC.
2. Mesmo texto copiado de novo depois de 2 s vira item novo e é reenviado (intenção do usuário: "copiar de novo").
   `dedupe_content` mantém uma linha por conteúdo em cada lado.
3. `created_at` de item drenado do inbox é o momento do drain, não da cópia (o `ts` original fica só no inbox).
4. Back na Activity raiz mata o processo (Tauri) → o IME renasce sem núcleo → tudo vai para o inbox até reabrir o app.
   Um FGS `connectedDevice` ou desacoplar o núcleo do Tauri resolveria; fora do escopo desta etapa.
5. `EXTRA_IS_SENSITIVE` continua sem app que a marque no emulador; caminho: `sensitive` → `excluded_by_source`
   → ignorado se `ignore_password_managers`.
6. Sem File Transfer, sem mudança no protocolo.
