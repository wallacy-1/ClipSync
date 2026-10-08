# ClipSync

Área de transferência sincronizada entre Windows e Android, direto pela rede local (P2P).
Sem servidor, sem conta, sem nuvem. Uma base de código: **Tauri 2 + React + TypeScript + Rust**.

```
 Windows / Android
   React (UI)  ──invoke/listen──▶  Rust (núcleo)
                                    ├── clipboard/   ClipboardProvider (Windows | Android)
                                    ├── content.rs   texto · link · código · e-mail · token · cartão
                                    ├── db.rs        SQLite local (histórico, dispositivos, fila)
                                    ├── identity.rs  chave X25519 estável por instalação
                                    ├── keystore.rs  chaves privadas cifradas em repouso
                                    ├── sync.rs      regras, fila offline, anti-loop, expiração
                                    └── transport/   mDNS · TCP · Noise (snow) · pareamento
```

## Como funciona

1. `clipboard::spawn_watcher` detecta uma cópia (Windows: número de sequência do clipboard; Android: polling + leitura ao voltar ao primeiro plano).
2. `content::classify` identifica o tipo e marca conteúdo sensível (tokens, cartões).
3. `sync::on_local_clip` aplica as regras (sync automático, tipos, pausa, dispositivos com "enviar automático").
4. O item vai por TCP dentro de uma sessão **Noise XX (X25519 + ChaChaPoly + BLAKE2s)** para cada dispositivo pareado; offline → fila em SQLite, enviada ao reconectar.
5. Quem recebe salva no seu SQLite, aplica no clipboard (se "receber automático") e **não reenvia** — o hash do que o ClipSync escreveu é ignorado pelo watcher, o que impede o ping-pong.

### Descoberta e pareamento

- Cada instância anuncia `_clip-sync._tcp.local` (mDNS/DNS-SD) com `id`, `name`, `platform`, `pk`.
- O PC gera um QR com identidade + endereços + **segredo aleatório de 256 bits**, e mostra também um código de 6 caracteres.
  O celular escaneia (ou digita o código); os dois rodam um **PAKE (SPAKE2)** com esse segredo e a chave resultante vira a PSK
  de um handshake **Noise XXpsk3**: sem o segredo, o handshake falha.
- PAKE porque o código tem só ~30 bits: usado direto como PSK, quem se passasse pelo PC (mDNS falso) testaria todos os códigos
  offline. Com SPAKE2 cada conexão testa um único palpite, e o PC encerra a sessão após **5 tentativas erradas**.
- Ao concluir, cada lado guarda a chave pública do outro. Conexões seguintes usam Noise XX e só aceitam chaves já pareadas. Remover um dispositivo revoga a chave.
- `deviceId = SHA-256(chave pública)[..16]`; o nome é só metadado.

### Segurança

- Nenhum clipboard trafega em texto puro; em claro só vão o `PreHello` (versão + modo) e, no pareamento, as mensagens do SPAKE2 (não revelam o segredo).
- Chave privada cifrada em repouso com ChaCha20-Poly1305. A KEK fica no **Credential Manager** (Windows) ou cifrada pelo **Android Keystore** (Android, `KeyGuard.kt`).
- Criptografia só via bibliotecas consolidadas: `snow`, `spake2`, `chacha20poly1305`, `sha2`.
- Panic no núcleo (o release usa `panic = "abort"`) é gravado em `crash.log` no diretório de dados do app
  (`%APPDATA%\app.clipsync.desktop\` no Windows) antes de o processo encerrar.

### Atualização automática (Windows)

`tauri-plugin-updater` procura uma versão nova 15 s após abrir e a cada 6 h; Ajustes → Atualização mostra/instala.
O pacote só é instalado se a assinatura bater com `plugins.updater.pubkey` (`tauri.conf.json`); com a chave vazia o
updater fica desligado. Android atualiza pela loja/APK.

### Eventos (Rust → React)

`device:discovered/connected/disconnected/updated` · `pairing:started/completed/failed` ·
`clipboard:changed/sent/received/expired` · `sync:started/completed/failed/state` · `settings:changed` · `tray:shown`
(Tauri não aceita `.` em nomes de evento.)

## ClipSync Keyboard (IME opcional)

Teclado Android próprio, **opcional e ativado só pelo usuário** nas Configurações do Android. Dois temas (`default`, `clipsync`).
Com o teclado selecionado, o IME recebe eventos de clipboard com o app fechado, tela bloqueada e pós-reboot
([`docs/ime-probe-clipboard.md`](docs/ime-probe-clipboard.md)) e entrega ao mesmo pipeline do app (`handle_text` → P2P);
sem o núcleo vivo, guarda num inbox selado pelo Keystore e drena ao abrir ([`docs/ime-bridge.md`](docs/ime-bridge.md)).
Detalhes, privacidade e validação: [`docs/ime.md`](docs/ime.md).

## Desenvolvimento

Pré-requisitos (Windows): Node 20+, Rust (rustup), **Visual Studio Build Tools** com "Desktop development with C++", WebView2 (já vem no Windows 11).
Android: Android Studio/SDK, NDK, JDK 17+, `ANDROID_HOME` e `NDK_HOME` definidos, targets Rust:

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

```bash
npm install
npm run dev                # só a UI no navegador, com mock (use ?platform=android, ?state=offline|empty|error|first)
npm run tauri dev          # app Windows completo
npm run tauri android init # uma vez: gera src-tauri/gen/android
npm run tauri android dev  # app Android (emulador/dispositivo)
npm run tauri build        # instalador Windows
npm run tauri android build
```

Testes de classificação de conteúdo: `cd src-tauri && cargo test`.

## Ambiente de build nesta máquina

`C:` está sem espaço; o target Rust e o cache Gradle vão para `E:\clipsync-build` (`build.sh`, `run.ps1`).
`run.ps1 <perfil> [portaCDP]` sobe uma instância debug com identidade/banco próprios (`CLIPSYNC_PROFILE`) —
usado para testar dois nós na mesma máquina. `scripts/cdp.mjs` invoca comandos Tauri na WebView real via DevTools Protocol.

## Android — comportamento real do clipboard (documentado, não contornado)

| Aspecto | Comportamento oficial |
|---|---|
| API | `ClipboardManager` (`getPrimaryClip` / `setPrimaryClip`), sem permissão de manifesto. |
| Android ≤ 9 | Qualquer app lê o clipboard a qualquer momento. |
| Android 10+ (API 29) | **Só o app em primeiro plano com foco de janela** (ou o teclado padrão) lê o clipboard; os demais recebem `null` sem erro. `OnPrimaryClipChangedListener` também deixa de disparar para apps sem foco. |
| Android 12+ | O sistema mostra um aviso "*app* colou da área de transferência" ao ler conteúdo de outro app. |
| Android 13+ | Clipboard limpo automaticamente (~1 h); `EXTRA_IS_SENSITIVE` marca conteúdo sensível (respeitado pelo ClipSync como "excluído pela origem" quando presente). |
| Escrita em background | Permitida (`setPrimaryClip`), sujeita a restrições de OEM. |
| Monitoramento contínuo em background | **Não é possível** sem ser o teclado padrão ou um serviço de acessibilidade. Acessibilidade fica fora do escopo (invasiva). Teclado: o ClipSync traz um IME **opcional** (`docs/ime.md`): selecionado como teclado padrão, ele detecta cópias com o app em segundo plano e as entrega ao núcleo. Foreground Service **não** dá acesso ao clipboard: **provado empiricamente** em `probes/fgs-clipboard` (Android 17): com FGS `dataSync` ativo e processo em `procState=FGS`, `hasPrimaryClip()` retorna `false`, `getPrimaryClip()` `null` e o listener nunca dispara enquanto a Activity não tem foco; volta a funcionar no primeiro poll ao recuperar o foco. |

Consequência para o ClipSync (implementado assim):
- **Detecção só com o app aberto**: polling em Rust (1,2 s) + leitura imediata ao voltar ao primeiro plano (`visibilitychange` → `clipboard_poll_now`).
- **Receber**: com o app em foreground o listener TCP recebe e escreve no clipboard. Em background (Android 15+) o sistema corta a rede do processo *cached*; os itens ficam na fila do PC e chegam ao reabrir.
- A arquitetura (`ClipboardProvider`) permite trocar a estratégia depois sem tocar na UI.

## Android — chave privada

`KeyGuard.kt` cria uma chave AES-256-GCM **não exportável** no Android Keystore. O Rust gera a KEK aleatória,
pede ao Keystore para cifrá-la (JNI, `ndk-context` + `jni`) e só grava `kek.sealed`. A chave privada X25519 fica em
`identity.bin`, cifrada com a KEK. Nada em texto puro em disco.

```
Windows → Credential Manager (keyring)      Android → Android Keystore (KeyGuard.kt)
```

## Estado da validação (2026-09-18)

Compilação: **Windows OK** (`cargo build`, `cargo test`, `cargo clippy -D warnings`) · **Android OK**
(`tauri android build --debug --apk`, `.so` alinhado a 16 KB, APK 49 MB instalado e executado no emulador
Pixel 8a · **Android 17 / API 37** · x86_64).

### Testado em dispositivo/emulador (REAL)

| Item | Estado | Evidência |
|---|---|---|
| Build Android | REAL | `app-universal-debug.apk`; LOAD segments 16384 |
| Instalação | REAL | `adb install` → Success (após limpar `app/build` — APK incremental do Gradle tinha 140 MB de lixo) |
| Runtime | REAL | app abre sem crash; `[IDENTITY]`, `[TRANSPORT] Listening on 0.0.0.0:47820`; UI do design (primeiro uso) |
| Android Keystore | REAL | `kek.sealed` = 60 B (IV 12 + KEK 32 + tag 16, AES-GCM do `KeyGuard`); `identity.bin` cifrado; `grep private_key` sem resultado; sem `kek.bin` |
| Identidade persistente | REAL | mesmo `device_id` após `force-stop` + reabrir e após reiniciar o emulador (decifra via Keystore) |
| Pairing PC↔Android (payload do QR) | REAL | código errado → `decrypt error` rejeitado; certo → pareado nos dois lados |
| P2P cifrado | REAL | Android conecta em `192.168.0.109:47820` (Noise XX, mesmo código do teste PC↔PC com relay) |
| Windows → Android | REAL | `Item sent` → `Item received/stored apply_clipboard=true`; releitura do clipboard Android bate com o hash escrito |
| Android → Windows | REAL | cópia externa no app Configurações do Android → `Change detected` no Android → PC `Item received from=a146…` e aplicou |
| Anti-loop | REAL | cada lado ignora a própria escrita (`Change ignored (written by ClipSync)`), sem reenvio |
| Desconexão/reconexão | REAL | reinício do emulador e ida para background → reconecta ao voltar (Android inicia pelo último endereço) |
| Fila offline → flush | REAL | Android: `Item queued` (em background) → `Flushing outbox count=1` → ack; PC idem |
| Revogação | REAL | remover Android no PC → `[AUTH] Rejected unpaired device`; Android fica `disconnected` |
| Pairing expirado | REAL | sessão de 3 min expirada → `código incorreto ou expirado` |
| Clipboard Android (API 37) | REAL | background: 12 s sem leitura; foreground: detecta em ≤ 2 s |

### Validado só por compilação/código (PARCIAL) ou bloqueado

| Item | Estado | Motivo |
|---|---|---|
| mDNS Windows ↔ Android | BLOQUEADO no emulador | NAT do emulador não passa multicast (nenhum lado descobre o outro). Código e `MulticastLock` compilam e rodam; prova exige aparelho físico na mesma Wi-Fi. Pareamento e reconexão funcionam pelo endereço do QR. |
| Escanear QR pela câmera | PARCIAL | plugin compila e a permissão está no manifesto; testado o mesmo comando (`pair_with_payload`) com o conteúdo real do QR, não a câmera. |
| Código digitado (Android) | PARCIAL | depende de mDNS para achar o PC; bloqueado no emulador pelo motivo acima. |
| aarch64 | PARCIAL | só x86_64 executado (emulador). |

### Limitações reais observadas no Android 15+/17

- **Background = sem rede**: ao ir para background, o processo entra em *cached state* e o Android fecha os sockets
  (PC vê `unexpected end of file`). O ClipSync reconecta em ~1 s ao voltar ao foreground e a fila entrega o que ficou pendente.
- **Clipboard só em foreground**: leitura retorna vazio sem foco de janela (comprovado). Nada de acessibilidade. Com o **ClipSync Keyboard** selecionado, o IME lê e entrega ao núcleo (`docs/ime-bridge.md`): Android → Windows automático com o app fechado, provado no emulador.
- Emulador compartilha o clipboard host↔guest (mesmo com `-feature -ClipboardSharing`), o que confunde testes direcionais; os testes acima isolaram a direção desligando "sync automático" no PC.
- Libs `libsurface_util_jni.so`/`libimage_processing_util_jni.so` (CameraX, via plugin de QR) ainda não são 16 KB-aligned; Android roda em modo compatível.

## Limitações conhecidas do MVP

- Clipboard: texto vira histórico; **imagem copiada** sincroniza pela transferência de arquivos e vai direto para o clipboard
  do outro lado (não entra no histórico). Ver `docs/roadmap.md` § 2.
- Android: **Compartilhar → ClipSync** envia arquivos e texto de qualquer app (`docs/roadmap.md` § 1).
- No Android, a leitura do clipboard só acontece com o app em primeiro plano (restrição do Android 10+); receber em segundo plano depende do processo estar vivo.
- mDNS no Android: `MainActivity` adquire `MulticastLock` (API oficial); como fallback, o QR carrega os endereços do PC e o app reconecta pelo último endereço conhecido.
- Exclusão por app de origem não existe (o Android não expõe a origem); vale "Ignorar gerenciadores de senha", que respeita `ExcludeClipboardContentFromMonitorProcessing` (Windows) e `EXTRA_IS_SENSITIVE` (Android 13+).
- Transporte só IPv4 (listener em `0.0.0.0`); IPv6 fica para depois.
- Duas instâncias na mesma máquina compartilham o clipboard do Windows — ambas detectam cada cópia (isso serviu de teste de loop mais agressivo).
