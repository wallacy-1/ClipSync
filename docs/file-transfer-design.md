# File Transfer P2P — proposta arquitetural (3c)

Data: 2026-09-19 · Estado: **aprovada; 3c.1–3c.4 implementadas** (ver "Estado da implementação" no fim) · Base: código após a Etapa 3b (IME → Rust → P2P).

## 1. Diagnóstico da arquitetura atual

- `transport/connection.rs`: 1 TCP por par; `PreHello` (v=1) → Noise XX/XXpsk3 → `Hello{identity}` → loop `select!`
  {rede, `mpsc` **unbounded** de `Message`, ping 20 s}; idle 70 s.
- `transport/protocol.rs`: quadro `[u32 len][bytes]`, `MAX_FRAME=65535`; `Message` é **JSON** fatiado em 60 000 B com
  1 byte "continua" (0/1) na frente de cada fatia; `MAX_MESSAGE=4 MiB`. Variante desconhecida derruba a conexão.
- Autenticação/autorização/revogação já vivem no handshake (chave estática ∈ `devices`, não bloqueado).
- Reconnect: loop 8 s + `maybe_connect`; `sync::on_connected` faz flush do `outbox`; não há hook `on_disconnected(device)`.
- `outbox(item_id, device_id)` é 100 % clipboard (payload no SQLite) — não reutilizar para arquivos; replicar o padrão.
- Android: sem plugin de dialog/fs; ponte JNI pronta (`android_jni::with_class`, `KeyGuard`, `ImeBridge`, `CoreBridge`).

Obstáculos concretos: (a) JSON não serve para bytes; (b) canal unbounded não dá backpressure; (c) falta hook de desconexão.

## 2. Protocolo

### Framing (um canal, dois tipos de quadro)
Byte 0 do plaintext de cada quadro Noise vira tag: `0` = última fatia JSON, `1` = fatia JSON continua (como hoje),
`2` = **quadro binário**: `[2][transfer_id 16 B][chunk_index u32][frag_index u16][frag_count u16][dados ≤ 59 975 B]`.
`recv()` devolve `Inbound::{Msg(Message), Data(DataFrame)}`. Clipboard permanece byte a byte idêntico.

### Versão/caps
`PROTOCOL_VERSION = 2`; `Hello.caps: Vec<String>` (`#[serde(default)]`, `["file-v1"]`). Só oferecer arquivo a quem anuncia a cap.

### Mensagens de controle (JSON em `Message`)
```
FileOffer   { transfer_id, name, mime?, size, chunk_size, total_chunks, sha256?, mtime? }
FileAccept  { transfer_id, offset }          // offset>0 = retomada; receptor é a autoridade
FileReject  { transfer_id, reason }          // declined|unknown|too_large|no_space|busy|disabled
FileAck     { transfer_id, received_through } // cumulativo, bytes duráveis (após fsync)
FileDone    { transfer_id, sha256 }          // emissor, hash calculado em streaming
FileResult  { transfer_id, ok, error? }      // receptor, após verificar e finalizar
FileCancel  { transfer_id, reason }
```
Dados só em quadros tag 2. Progresso/checkpoint = offset em bytes (gravação sequencial). Sem ack por chunk: TCP+Noise
garantem ordem/integridade; ack = flow control + checkpoint.

### IDs
`transfer_id` UUID v4; chave lógica `(device_id da sessão, transfer_id)`; ids de outro device são ignorados.

### Estados
```
Queued → Offered → Transferring → Verifying → Completed | Failed
Receptor: Incoming → Transferring → Verifying → Completed | Failed
Qualquer ativo —conexão cai→ Waiting —reconecta→ Offered(offset)
Cancelled{by} de qualquer não-terminal. Terminais: Completed, Failed{error}, Cancelled{by}.
```
Sem `Paused` manual no MVP. Aceite: automático se `device.auto_receive`, senão prompt.

### Chunk, janela, ACK
Chunk lógico **256 KiB** (= 5 quadros Noise); janela **8 chunks (2 MiB)**; receptor faz `fsync` + `UPDATE bytes_done` +
`FileAck` a cada 4 chunks (1 MiB) ou 250 ms e no fim; ack timeout 30 s → fecha socket → caminho normal de desconexão.
Comparativo: 16/32 KiB = índice e acks enormes; 64 KiB = 1 chunk/quadro mas checkpoint frequente demais; 256 KiB = 20 480
chunks em 5 GiB, janela 2 MiB, resume perde ≤ 1 MiB; ≥ 1 MiB não ganha nada em LAN e custa RAM.

## 3. Persistência

```sql
CREATE TABLE file_transfers (
  id TEXT PRIMARY KEY, direction TEXT NOT NULL, device_id TEXT NOT NULL,
  original_name TEXT NOT NULL, safe_name TEXT NOT NULL, mime TEXT,
  size INTEGER NOT NULL, chunk_size INTEGER NOT NULL,
  sha256_expected TEXT, sha256_actual TEXT,
  state TEXT NOT NULL, state_reason TEXT,
  bytes_done INTEGER NOT NULL DEFAULT 0,
  source TEXT, temp_path TEXT, final_path TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, completed_at INTEGER
);
CREATE INDEX idx_ft_device_state ON file_transfers(device_id, state);
```
Sem tabela de chunks (entrega sequencial → `bytes_done` basta). 1 `UPDATE` por ack (~1 MiB).
Filesystem: receptor `.part` em `<Downloads>\ClipSync\.part\<id>.part` (Windows, mesma unidade → rename atômico) ou
`files/transfers/<id>.part` (Android). Emissor: nada além da linha. Memória O(1): janela 2 MiB + buffers Noise.
"Outbox": a própria tabela (`state IN (queued, waiting)` por device); `transfer::on_connected(device)` retoma.
Tabela `outbox` do clipboard não muda.

## 4. Android

- Escolher: `ACTION_OPEN_DOCUMENT` via Kotlin (`FileBridge.pickFile`, `MainActivity` activity result), `takePersistableUriPermission`,
  URI guardada em `source`.
- Ler: `openFileDescriptor(uri,"r").detachFd()` → Rust `File::from_raw_fd` (streaming, seek, sem cópia). Fallback se não
  seekável: copiar para `files/transfers/out-<id>` antes de oferecer. URI revogada → `Failed{source_unavailable}`.
- Receber: `.part` privado → finalizar em `MediaStore.Downloads` (`IS_PENDING=1` → copiar → `IS_PENDING=0`,
  `RELATIVE_PATH=Download/ClipSync/`). Sem permissões em API 29+; API 26–28: `getExternalFilesDir(DOWNLOADS)`.
  Otimização futura: gravar o `.part` direto no item pendente (elimina a cópia).
- Background: processo vivo com IME selecionado (`procState=7`); sem IME, Android 15 corta rede em *cached* →
  **FGS `dataSync` só durante transferências** (iniciado do foreground, notificação com progresso e ação Cancelar,
  encerra sozinho sem transfers ativas) — **parte obrigatória** da implementação Android, não otimização.

## 5. Windows

`tauri-plugin-dialog` para picker; drag & drop via `on_drag_drop_event` (MVP+1); Explorer "Enviar para ClipSync" via
registro + `--send <path>` (depois). Destino `Downloads\ClipSync\`. Sanitização: `<>:"/\|?*`, controles, `..`, ponto/espaço
finais, reservados (`CON PRN AUX NUL COM1-9 LPT1-9`), ≤ 200 B; colisão → `nome (2).ext`.

## 6. Segurança

Só sessões Noise autenticadas (device pareado, não bloqueado, com cap). Revogar/bloquear → `cancel_all(device)` + `.part`
apagado. `transfer_id` aleatório e escopado por device. `safe_name` sanitizado; `final.parent() == dest_dir` verificado;
`.part` nomeado só pelo id. MIME = metadado (validado contra extensão; senão `application/octet-stream`); nada executa.
`size ≤ MAX`, `total_chunks == ceil(size/chunk)`, espaço livre ≥ `size×1,05 + 64 MiB`, comprimento final == `size`,
nunca escrever além de `size`; quadro fora de ordem/id desconhecido → `FileCancel{protocol}`.

## 7. Performance (estimativa)

| Arquivo | Chunks | Quadros | LAN ~30 MB/s | SHA-256 | Obs. |
|---|---|---|---|---|---|
| 10 MB | 40 | 170 | < 1 s | — | latência de Offer/Accept domina |
| 100 MB | 400 | 1 700 | 3–5 s | 0,1–0,5 s | |
| 1 GB | 4 096 | 17 500 | ~35 s | 1–5 s | + cópia MediaStore 3–10 s |
| 5 GB | 20 480 | 87 000 | ~3 min | 5–25 s | + cópia MediaStore 30–60 s |

Gargalos: Wi-Fi; ChaCha20-Poly1305 por quadro no Android; cópia final MediaStore. SQLite e JSON irrelevantes.

## 8. Recovery

| Evento | Emissor | Receptor |
|---|---|---|
| Conexão cai | aborta task, `Waiting`; reconectou → reenvia `FileOffer` | trunca `.part` a `bytes_done`, `Waiting`; responde `FileAccept{offset}`; re-hasheia prefixo |
| Crash/reboot | linhas ativas → `Waiting` na subida; reabre `source`; hasheia 0..offset local | idem; `.part` órfão apagado; linha sem `.part` reinicia |
| Origem sumiu | `Failed{source_unavailable}` + `FileCancel` | apaga `.part` |
| Hash diverge | `attempts+1`; < 2 → reoferece do 0; senão `Failed{checksum}` | apaga `.part`, `Failed{checksum}` |
| Abandono | `Waiting` > 7 dias, 2 falhas de hash, peer removido | idem |

## 9. Arquivos

**Reutilizar**: identity, keystore, noise, discovery, pairing, reconnect, devices/revogação, events, comandos, Db, todo `clipboard/*`.

**Criar**: `transfer/{mod,state,sender,receiver,store,names,dest,android}.rs`; `FileBridge.kt` (+ `TransferService.kt` em 3c.5);
React `TransfersView` (Win/Android), `SendFileSheet`, slice no store; `docs/file-transfer.md`.

**Alterar**: `protocol.rs` (tag 2, `Inbound`, `File*`, caps, v2), `connection.rs` (despacho, canal bounded de dados com
prioridade ao controle, hook `on_disconnected`), `state.rs` (`ConnHandle{tx,data_tx,caps}`, `transfers`), `db.rs` (schema),
`lib.rs`, `commands.rs`/`transfer/commands.rs`, `events.rs`, `sync.rs` (1 chamada), `Cargo.toml` (`tauri-plugin-dialog`,
tokio `fs`), capabilities, `models.rs`/`types.ts`/`ipc.ts`/`mock.ts`/`app.ts`/shells; manifest só em 3c.5 (FGS).

## 10. Plano incremental

| Etapa | Entrega | Prova |
|---|---|---|
| 3c.1 | protocolo + estados + nomes + schema/store | testes unitários; clipboard inalterado |
| 3c.2 | Windows ↔ Windows sem UI (sender/receiver/manager/comandos/eventos) | `run.ps1 a/b` + `cdp.mjs`: 0 B, 1 KB, 100 MB; cancel; `relay.mjs` corta a ~40 % de 500 MB → resume → hash ok |
| 3c.3 | Windows → Android (MediaStore) | arquivo em Downloads/ClipSync; resume com HOME |
| 3c.4 | Android → Windows (SAF fd) | PC recebe; URI revogada → `Failed` limpo |
| 3c.5 | lifecycle Android (FGS `dataSync` durante transferência, sweep, reboot) | 500 MB em background; reboot no meio → retoma |
| 3c.6 | UI Windows + Android | wireframes; mock |
| 3c.7 | endurecimento (limites, retenção, drag & drop, docs, matriz) | matriz verde |

## 11. Limites e matriz de testes

Limites MVP: 4 GiB · chunk 256 KiB · janela 8 · 1 ativa/device · 20 na fila/device · espaço ≥ `size×1,05+64 MiB` ·
ack timeout 30 s · 2 tentativas por checksum · `Waiting` expira em 7 dias · retenção 30 dias · nome ≤ 200 B ·
`total_chunks ≤ 16 384` · 0 B permitido.

Matriz: 0 B; 1 KB/10 MB/100 MB; 500 MB com corte a 40 % → resume; nomes maliciosos/reservados/longos/vazios; Unicode;
colisão; checksum incorreto (via `FileDone` falso); quadro duplicado/fora de ordem; cancel local/remoto; expiração 7 dias;
reboot Android/PC; device revogado; device sem cap; falta de espaço; 2 transfers mesmo device (fila) e 2 devices (paralelo);
clipboard durante 1 GB (< 1 s); Android URI revogada; destino visível em Downloads.

## 12. Riscos / experimentos prévios

| Risco | Experimento |
|---|---|
| R1 SAF fd não seekável / `detachFd` no Rust | probe JNI: 3 providers, `lseek` + ler 1 MiB |
| R2 rede em background sem IME/FGS | probe 100 MB com HOME sem IME → confirma 3c.5 |
| R3 throughput Noise no Android | microbench `snow` 1 GB |
| R4 MediaStore `IS_PENDING` + crash | probe: matar processo com item pendente |
| R5 canal unbounded/prioridade | resolvido por design; validar no teste "clipboard durante 1 GB" |
| R6 bump de versão derruba peers v1 | aceito (instalações atualizadas juntas); `caps` evita repetir |
| R7 Downloads em outra unidade/OneDrive | `.part` no mesmo pai do destino; fallback copy+delete |
| R8 flash lenta no Android → ack timeout falso | timeout 30 s + fsync/1 MiB; medir em R3 |

Críticos antes de implementar: R1 e R2 (≈1 h de probes), na abertura de 3c.3/3c.4; não bloqueiam 3c.1–3c.2.

---

## Estado da implementação

### 3c.1 — feito (2026-09-20)
- `transport/protocol.rs`: `PROTOCOL_VERSION = 2`; tags de quadro `0/1` (JSON, inalterado) e `2` (`DataFrame` binário,
  cabeçalho 25 B, até 59 975 B de dados); `Hello.caps` (`serde(default)`, peers v1 continuam parseando); `Message::File*`
  (`FileOffer{sha256: Option}`, `FileAccept{offset}`, `FileReject{reason}`, `FileAck{received_through}`, `FileDone{sha256}`,
  `FileResult`, `FileCancel{reason}`); `SecureChannel::{send_data, recv_any}` com `Inbound::{Msg, Data}`; `recv()` mantido
  para o handshake de aplicação.
- `transport/connection.rs`: envia/recebe `caps` (por ora vazio — só anunciaremos `file-v1` em 3c.2), loop usa `recv_any`;
  quadros de dados e mensagens `File*` são apenas logados (debug) até o gerenciador existir. Nada do caminho de clipboard mudou.
- `state.rs`: `ConnHandle.caps`.
- `transfer/state.rs`: `Direction`, `TransferState` (queued/offered/incoming/transferring/verifying/waiting/completed/failed/cancelled),
  `Event`, `transition()` pura; `transfer/names.rs`: `sanitize`, `dedupe_name`, `resolve_in`; `transfer/store.rs`: tabela
  `file_transfers` + CRUD/fila/sweep/purge; `transfer/mod.rs`: limites (chunk 256 KiB, janela 8, ack a cada 4, 4 GiB, 16 384 chunks,
  20 abertas/device, ack timeout 30 s, margem 64 MiB, 2 tentativas de checksum) e `validate_offer`.
- SHA-256 conforme aprovado: `FileOffer.sha256` opcional (fonte seekável/barata → calculado antes; senão `null` e obrigatório em `FileDone`).
- Testes: 36 verdes (`cargo test`), `clippy --all-targets -D warnings` limpo; inclui sessão Noise real em loopback multiplexando
  JSON fatiado + quadro binário + JSON. Clipboard PC v2 ↔ Android v2 (IME → PC, PC → Android, anti-loop) revalidado no emulador.
- Consequência do bump: PC v2 rejeita Android v1 e vice-versa ("versão de protocolo incompatível") — atualizar os dois lados juntos.

### 3c.2 — feito (2026-09-20): Windows ↔ Windows

**Código**: `transfer/manager.rs` (orquestração, fila por device, roteamento, `on_connected/on_disconnected`, `send_file/cancel/resume/list`),
`transfer/sender.rs`, `transfer/receiver.rs` (+ `FrameSeq`, validador puro da sequência de quadros), `transfer/dest.rs`
(Downloads/ClipSync, `.part` no mesmo volume, espaço livre via `GetDiskFreeSpaceExW`, finalização atômica), `transfer/hash.rs`;
`connection.rs` anuncia `file-v1` (desktop), canal **bounded** de dados (`mpsc::channel(8)` quadros) além do canal de controle
unbounded, `select!` **biased** (rede → controle → ping → dados), `conn_id` por conexão, hooks do manager; `SecureChannel::recv_any`
virou **cancel-safe** (leitura resumível — bug encontrado no teste de 500 MB: a leitura era cancelada no meio de um quadro pelo
`select!` e a conexão caía com "quadro maior que o permitido"). Comandos: `transfer_send/list/cancel/resume/peer_supports`;
eventos `transfer:state`, `transfer:progress` (≤ 4/s). Revogar/bloquear device cancela tudo e avisa o peer antes do `Bye`.

**Testes reais (dois perfis Windows na mesma máquina, build debug)**: 0 B, 1 KiB, 256 KiB (= 1 chunk), 1 000 000 B (4 chunks),
10 MB, 100 MB (b→a), 500 MB — SHA-256 igual nos dois lados em todos; Unicode (`relatório ✓ 日本.bin`), sem extensão, `.tar.gz`,
colisão (`1k (2).bin`, `1k (3).bin`); cancel pelo emissor e pelo receptor (`.part` apagado, nenhum dado depois); cancel durante
`waiting` (peer avisado na reconexão via `state_reason=local-pending-notify`); 2 transferências para o mesmo peer (fila
sequencial) e 2 direções simultâneas; peer sem `file-v1` (Android) fica `queued`; device não pareado → erro; checksum incorreto
(fonte alterada após o hash) → `failed checksum:mismatch` nos dois lados, `.part` apagado, nada em Downloads; bloqueio no meio →
`cancelled/system` + `cancelled/remote`; **500 MB com receptor morto a 40 % (208 MB)** → sender `waiting` → receptor religado
(`Startup sweep`) → `FileAccept{offset=208 666 624}` (fronteira de chunk, autoridade do receptor) → `completed`, SHA igual, `.part`
sumiu; **emissor morto a 30 %** → retoma de 35 389 440 → SHA igual; clipboard durante 500 MB: `Item received` no mesmo segundo.
Memória: pico 42–44 MB (WorkingSet) nos dois processos durante 500 MB (base ~40 MB). Throughput: ~4 MB/s em **debug** (ChaCha20-Poly1305
+ SHA-256 sem otimização); medir em release antes de tirar conclusões.

**Limitações conhecidas**: aceite só automático (`auto_receive`), sem prompt; sem retomada automática após `failed` (usar
`transfer_resume`); dois perfis na mesma máquina disputam a porta 47820 (artefato de teste, não de produto); throughput debug.

### 3c.3 — feito (2026-09-20): Windows ↔ Android

**Kotlin (`app/src/main/java/app/clipsync/desktop/transfer/`)**: `FileBridge.kt` (SAF `ACTION_OPEN_DOCUMENT` + `takePersistableUriPermission`,
`describe`, `openRead` → `ParcelFileDescriptor.detachFd()` → Rust é dono do fd, `freeSpace` via `StatFs`, `publishToDownloads` via
`MediaStore.Downloads` com `IS_PENDING=1 → cópia → IS_PENDING=0` em `Download/ClipSync`, `cleanupPending` na subida), `TransferService.kt`
(FGS `dataSync` **só durante transferência ativa**: notificação com progresso e ação Cancelar, `stopSelf` ao ficar sem transfers;
contrato `startForegroundService → startForeground` respeitado em START e em STOP pendente; recusa do sistema não é fatal),
`TransferBridge.kt` (fgsStart/Update/Stop + `nativeCancel` do botão da notificação). `MainActivity.onActivityResult` entrega o resultado do
seletor. Manifest: `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_DATA_SYNC`, `POST_NOTIFICATIONS`, `<service … foregroundServiceType="dataSync">`.
`proguard-rules.pro`: keep das classes JNI (release era quebrado desde o KeyGuard).

**Rust**: `transfer/source.rs` (`TransferSource::{Path, AndroidUri}`, `describe`, `open` → `std::fs::File` seekável; URI não-seekável →
cópia única para disco privado, nunca RAM), `transfer/android.rs` (JNI para FileBridge/TransferBridge + `nativeCancel`),
`dest.rs` (Android: `.part` em `<app_data>/transfers/.part`, destino = MediaStore, espaço exigido 2× porque a publicação é cópia),
`sender.rs` usa `source::open` + hash sobre o fd (`try_clone`), `receiver.rs` finaliza por `dest::finalize_platform`, `manager.rs`
liga/atualiza(≤1/s)/desliga o FGS e chama `cleanupPending` na subida; `commands.rs::transfer_pick_file`; `connection.rs` anuncia
`file-v1` também no Android. Protocolo v2 intocado.

**Testes reais** (emulador Pixel 8a Android 17 para funcionais; Caderno Android 16 para 100 MB/background/release por falta de espaço em disco no primeiro):

| Teste | W→A | A→W |
|---|---|---|
| 0 B / 1 KiB / 1 000 000 B / Unicode / sem extensão / colisão | ✅ SHA igual; colisão vira `1k (1).bin` (MediaStore) | ✅ SHA igual; colisão vira `1k (4).bin`… (Windows) |
| 10 MB / 60 MB / 100 MB | ✅ | ✅ (100 MB pelo caminho privado; 60 MB via **URI SAF**) |
| Kill do processo Android a ~40 % (receptor) / ~30 % (emissor) | ✅ retomou de 29 097 984 (autoridade do receptor), publicou, SHA ok | ✅ retomou de 33 030 144 (kill) e de 20 185 088 após **reboot** com URI persistida |
| Cancel | ✅ pelo botão da **notificação** do FGS: `cancelled/local` ↔ PC `cancelled/remote`, `.part` apagado, FGS parou | ✅ `transfer_cancel` no Android: PC `cancelled/remote`, `.part` apagado |
| URI removida/revogada entre morte e religada | — | ✅ `failed: source unavailable: URI sem permissão ou arquivo removido`, peer `cancelled/remote`, registro preservado |
| Espaço insuficiente | ✅ `FileReject{NoSpace}` antes de qualquer byte (`rejected:nospace`) | — |
| Tela bloqueada (app em background) | ✅ 20 MB recebido e publicado com a tela desligada | — |
| HOME durante transferência | ✅ FGS mantém (`procState` FGS); no Android 17 com o IME selecionado o FGS até sobe do background (`code:CURRENT_INPUT_METHOD`) | ✅ |
| Clipboard durante 100 MB (release) | PC→Android `Item received` no mesmo segundo | Android IME→PC no mesmo segundo |
| Back na Activity raiz | processo morre (Tauri) → `waiting` → reabrir retoma | idem (retomou de 37 224 448) |

**Release (100 MB, emulador Caderno x86_64 + PC release)**: W→A 19,6 s (~5,3 MB/s) + 0,5 s verificação/publicação; A→W 42 s (~2,4 MB/s).
Limite é a rede virtual do emulador (slirp), não o código — medir em aparelho físico. Memória: PC pico 31–35 MB; Android PSS ~96–100 MB (inclui WebView).
`.part`: cresce até `size` no privado; some após publicar.

**Problemas encontrados e corrigidos**: `ForegroundServiceDidNotStartInTimeException` (UPDATE via startForegroundService sem startForeground; STOP
chegando com START pendente); `ForegroundServiceStartNotAllowedException` ao parar do background (crash → agora tolerado);
NotificationService descartando (>5 notificações/s → throttle 1/s + `notify` em vez de `startForeground` nas atualizações);
ENOSPC na publicação (espaço exigido 2× no Android); R8 removendo classes JNI no release (keep rules); `cdp.mjs` sem página com subtítulo no release.

**Limitações**: (1) transferência **nova** chegando com o app em background e sem IME/FGS ativo: Android 15+ deixa o processo *cached* e corta a
rede → fica `queued` no emissor até o usuário reabrir (FGS não pode nascer do background; com o ClipSync Keyboard selecionado o sistema permite);
(2) Back/swipe na Activity raiz mata o processo mesmo com FGS ativo (Tauri) → `waiting`, retoma ao reabrir; (3) publicação no MediaStore é
uma cópia (2× espaço, tempo extra) — otimização futura: gravar o `.part` direto no item pendente; (4) release Android sem WebView depurável:
testes A→W em release usaram fila criada em debug; (5) emulador Pixel 8a com ~150 MB livres limitou os tamanhos nesse aparelho.

### 3c.4 — feito (2026-09-20): lifecycle Android (hardening, sem feature nova)

Objetivo: validar e corrigir lifecycle/background do File Transfer no Android antes da UI final. Protocolo, framing, `DataFrame`,
`CHUNK_SIZE`/`WINDOW_CHUNKS`, Noise, pairing, state machine, clipboard, IME e sender/receiver Windows **intocados**.

**Arquivos alterados**: `TransferService.kt` (reescrito o contrato com o sistema), `TransferBridge.kt` (`fgsUpdate/fgsStop` sem `Context`,
`onActivityResumed` → `nativeActivityResumed`), `MainActivity.kt` (`onResume` → `TransferBridge.onActivityResumed()`), `transfer/android.rs`
(JNI correspondente + `fgs_running` + `nativeActivityResumed`), `transfer/manager.rs` (`fgs_retry_if_needed`; sweep de `.part` órfão considera
só linhas **não terminais** como donas; limpa `out-<uuid>.tmp`), `transfer/dest.rs` (`remove_orphans_in`, `remove_outgoing_tmp` + 2 testes),
`scripts/lc.sh`, `scripts/android-cdp.sh` (helpers de teste). Docs: esta seção.

**Problemas encontrados / corrigidos**
1. **FGS fantasma após swipe/kill**: com FGS ativo, ao matar o processo o sistema recria `TransferService` (`Scheduling restart … for start-requested`)
   num processo **sem núcleo Rust** (só `onCreate`; o `onStartCommand` pode nem chegar) e repõe a notificação antiga → FGS "Recebendo arquivo" eterno
   sem transferência. Corrigido: `onCreate` sem `start()` neste processo (`desired=false && pendingStarts==0`) → `startForeground` (contrato) + `stopNow`.
   Um START reentregue sem `pendingStarts` também é encerrado. Validado (debug e release): `isForeground` some em < 1 s.
2. **UPDATE/STOP via `startService()`** (fonte dos `ForegroundServiceDidNotStartInTimeException`/`…StartNotAllowedException` da 3c.3): agora só o START
   passa por `startForegroundService()`; UPDATE e STOP falam direto com a instância (`main.post`). "Último pedido vence" (`desired`): START/STOP fora
   de ordem convergem; START que chega depois de STOP cumpre `startForeground` e para. Nada lança; recusa do sistema → transferência segue sem FGS.
3. **`.part` órfão com linha terminal** (`completed`/`cancelled`/`failed`) não era removido (a lista "conhecida" incluía terminais). Corrigido; caso 4
   (sem linha) já era tratado. `out-<uuid>.tmp` (cópia de origem não-seekável) deixado por processo morto agora é limpo na subida.
4. **FGS recusado em background → retry**: `MainActivity.onResume` pede ao Rust para subir o FGS se há transferência ativa sem ele. Defensivo:
   no Android 16 do emulador o start foi permitido em todos os cenários testados (`PROC_STATE_TOP`, `ACTIVITY_STARTER` ≈10 s após sair,
   `CURRENT_INPUT_METHOD` com o teclado selecionado) — o caminho não foi acionado na prática.
5. Piso de 250 ms entre `notify()` no Kotlin, além do ≤ 1/s por transferência no Rust: 2 transferências simultâneas → pico medido 3/s (limite ~5/s).

**Comportamento medido (emulador Caderno, Android 16 / API 36, targetSdk 36)**

| Evento | Comportamento | Transferência |
|---|---|---|
| **Home** | Activity `stopped`, processo vivo. Com FGS: `adj=50`, rede viva. Sem FGS e sem IME: `LAST_ACTIVITY` (adj 700) → *cached* (adj 900) em ~15–45 s; sockets recebem `ECONNABORTED` e novas conexões dão timeout | continua (FGS) / `queued` no emissor até reabrir |
| **Back** (app aberto pelo launcher) | Android 12+: `moveTaskToBack` — Activity `stopped`, processo vivo, FGS segue. **Igual a Home.** | continua; 500 MB completou após Back |
| **Back** (app aberto por `adb am start`/shell) | `finish()` (`wm_finish_activity … app-request`) → `onDestroy` → tao `std::process::exit` → processo morre com FGS | `waiting` → reabrir retoma |
| **Swipe nos recentes** | `wm_destroy_activity … remove-task` → mesmo caminho: **processo morre** (Tauri/tao sai no `WindowEvent::Destroyed`; `prevent_exit` não é opção — a Activity nova teria outro id e nenhuma WebView) | `waiting` (`bytes_done` na fronteira de chunk) → reabrir retoma; sem FGS fantasma |
| **Tela bloqueada** | FGS mantém; `mWakefulness=Asleep` por 30 s | continuou e completou |
| **Processo morto (`kill -9`)** — receptor 57 %, emissor 30 % e 50 % | `TransferRecord` fica; `.part` fica (truncado ao offset durável na retomada); `out-*.tmp` limpo; FGS não sobra; nenhuma task registrada | reabrir → `Startup sweep → waiting` → resume → SHA igual |
| **Reboot** — receptor a 57 % e emissor (URI SAF) a 50 % | app não sobe sozinho (sem serviço always-on); nada assume conclusão | reabrir → resume → SHA igual (URI persistida sobreviveu ao reboot) |
| **Nova transferência com app cached** (Activity fechada, sem IME, sem FGS) | Android corta a rede; o emissor fica `queued` (não `failed`) | reabrir → conecta → transfere → SHA igual |

**FGS lifecycle final**: `register()` → `startForegroundService(START)` → `onStartCommand` → `startForeground` (1 notificação, progresso ≤ 1/s por
transferência, piso 250 ms global, ação Cancelar) → última task `unregister()` → `stop()` → `stopForeground(REMOVE)+stopSelf`. 2 transferências
simultâneas: uma notificação, FGS vive até a última terminar (medido: 500 MB in + 100 MB out). Notificação some ao concluir/cancelar/falhar (sem
notificação de "concluído" — UI final). Arquitetura mantida: IME = clipboard em background; FGS `dataSync` = transferência ativa; nenhum depende do outro.

**`.part`**: caso 1 (`waiting`) mantém e retoma; casos 2/3 (`completed`/`cancelled`) e 4 (sem linha) removidos na subida ("Removed 3 orphan .part");
arquivo sem extensão `.part` no diretório não é tocado. **MediaStore pending**: kill durante a cópia final (linha `IS_PENDING=1` + `.pending-…-500mb.bin`
de 111 MB) → subida: `cleanupPending` remove linha e arquivo → `.part` (íntegro, `size` bytes) → `FileAccept{offset=size}` → re-hash → publica →
SHA igual. `cleanupPending` existente foi suficiente (nenhuma segunda implementação). **URI revogada/removida** entre morte e religada: `failed:
source unavailable: URI sem permissão ou arquivo removido`, linha preservada, peer `cancelled/remote` (`FileCancel{SourceUnavailable}`), sem panic.

**Cancel pela notificação** (debug e release): receptor Android (31 %) → `nativeCancel → manager::cancel → Ctl::Cancel → FileCancel{User}` → PC
`cancelled/remote`, `.part` apagado, FGS parou, notificação sumiu; emissor Android (durante o hash da origem) idem; `waiting` offline (PC morto) →
`cancelled/local`, aviso ao reconectar (`local-pending-notify`) → PC `cancelled/remote`; cancel repetido (3× Android + PC) → mesmo estado, sem erro.

**IME**: todos os cenários acima rodados **sem** o ClipSync Keyboard (Gboard selecionado): W→A, A→W, Home, kill, reboot, cancel, URI. **Com** o
teclado: nada muda negativamente; bônus: FGS permitido do background (`CURRENT_INPUT_METHOD`) e processo não vira *cached*. Clipboard durante 500 MB
com FGS + Activity fechada: PC→Android `Item received` + `Writing to system clipboard` no mesmo segundo (sem IME); Android IME→PC no mesmo segundo
(com IME); PC→Android com IME: emulador espelha o clipboard host↔guest (artefato já documentado em `ime-bridge.md`), 1 troca em cada sentido e
**nenhum ping-pong** depois. Transferência não bloqueou o clipboard.

**Release**: build limpo (`gradlew clean` + target Rust release apagado), R8 manteve `KeyGuard`, `FileBridge`, `TransferBridge`, `TransferService`,
`nativeCancel`, `nativeActivityResumed`, `cleanupPending`, `publishToDownloads` (verificado no DEX); assinado com o keystore de debug. Testes em
release: A→W 500 MB retomado em background (~3 MB/s no emulador), W→A 500 MB + swipe (sem FGS fantasma) + reabrir + cancel pela notificação,
W→A 100 MB com clipboard durante; 0 `FATAL`. Espaço insuficiente (disco cheio): `FileReject{NoSpace}` → `failed rejected:nospace`, 0 B gravados.
Windows ↔ Windows (a→b 100 MB) SHA igual. `cargo test` 49 ok · `clippy --all-targets -D warnings` limpo.

**Limitações (Android, não bug nosso)**: (1) swipe/`finish()` mata o processo (tao) — retomável ao reabrir; (2) sem IME e sem FGS o processo vira
*cached* e perde rede: transferência nova fica `queued` no emissor até o usuário reabrir — sem hack, sem serviço always-on; (3) sem reinício
automático após reboot (por design); (4) `POST_NOTIFICATIONS` negado (Android 13+) → FGS roda, notificação/Cancelar invisíveis (pedido de permissão
é assunto da UI final); (5) emissor faz SHA-256 da origem antes do `FileOffer` — em debug 500 MB levam ~10 s com a notificação em 0 %; se o
Android estiver cached nesse intervalo a conexão cai e a oferta nem sai (fica `queued`, retenta na próxima conexão).
