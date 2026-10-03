# Roadmap de produto

Levantamento de 2026-10-01. O núcleo (clipboard de texto P2P e transferência de arquivos Windows ↔ Android) está
validado no emulador (`README.md`, `docs/file-transfer-design.md`). O que falta é o que faz o app virar hábito.

Legenda: **P1** muda a percepção de "funciona" · **P2** confiança e acabamento · **P3** validação.

## Fluxo de uso

### 1. Compartilhar → ClipSync (Android) — P1 · feito (2026-10-01)
- `intent-filter` `ACTION_SEND` / `ACTION_SEND_MULTIPLE` (`*/*`) na `MainActivity`; `share/ShareBridge.kt` traduz o Intent em
  `{files, text}`; `share_take` entrega à UI (ao abrir, ao voltar ao primeiro plano e por `share:received` com o app já aberto).
- Arquivos vão pelo caminho do seletor SAF (`TransferSource::AndroidUri`); texto vira item de histórico e vai por `share_text`.
- Folha "Enviar para" sempre aparece (um toque); dispositivo offline é aceito e entra na fila.
- Validado no emulador (Android 17): Arquivos → Compartilhar → ClipSync com 2 PNGs → PC, SHA-256 igual; texto → PC.
- Limitação: permissão de URI de share é temporária; se o processo morrer no meio, a retomada falha com
  `source unavailable` (já tratado).

### 2. Imagem no clipboard — P1 · feito (2026-10-01)
- `clipboard/image.rs`: imagem copiada vira PNG em `<app_data>/clip-images` e segue pela transferência de arquivos com
  `FileOffer.purpose = "clipboard"` (capability `clip-image-v1`). Quem recebe finaliza no diretório privado (não em Downloads)
  e põe a imagem no clipboard se "receber automático" vale.
- Windows: leitura/escrita pelo plugin (arboard), PNG via crate `png`; anti-loop pelo número de sequência da própria escrita.
- Android: `clip/ClipImageBridge.kt` (`peek` só lê a descrição; `copyAsPng`; `setImage` via FileProvider com rótulo "ClipSync").
- Categoria "Imagens" nos tipos sincronizados (ligada por padrão; configurações antigas recebem via `types_rev`).
- Só a imagem mais recente fica na fila de cada dispositivo (as anteriores são canceladas). Limite: 64 MB de PNG.
- Validado no emulador: PC → Android (`image/png` no clipboard do Android) e Android → PC (pixels idênticos), sem eco.
- Limitações: Android só lê com o app em foco (como o texto); o IME não captura imagens; imagens não entram no histórico,
  só em Arquivos. No emulador o compartilhamento de clipboard host↔guest troca a imagem por texto — testar com
  "Clipboard sharing" desligado.

### 3. Arquivo copiado no Explorer (Ctrl+C) — P2 · feito (2026-10-01)
- `clipboard/files.rs` lê `CF_HDROP` (arboard `file_list`) quando o clipboard não tem texto e emite `clipboard:files`.
  **Nunca envia sozinho** (Ctrl+C de arquivo é quase sempre para colar em outra pasta): a UI oferece "Enviar para…" num
  cartão no canto da janela e no topo do painel rápido; um clique envia pela transferência (`transfer_send` por arquivo).
- Pastas ficam de fora (avisado); no máximo 20 arquivos por oferta; conectados aparecem antes dos offline (fila).
- A oferta some ao copiar outra coisa, ao dispensar, ao enviar ou depois de 10 min.
- Validado: `SetFileDropList` com 2 PNGs + 1 pasta → oferta "2 arquivos copiados · 1 pasta fica de fora" → Android, SHA igual.
- Limitação: com a janela fechada e sem notificação do Windows (item 5), a oferta só aparece ao abrir a janela/painel.

### 4. Aceitar/recusar arquivo recebido — P2 · feito (2026-10-01)
- Ajuste "Arquivos recebidos": **Sempre aceitar** / **Perguntar acima de 25 MB · 100 MB (padrão) · 500 MB · 1 GB** / **Sempre perguntar**
  (`Settings.file_accept` + `file_auto_accept_mb`). Imagem do clipboard não passa por aqui.
- "Receber automático" desligado (global, do dispositivo ou pausa) agora **pergunta** em vez de recusar calado.
- Pedido: cartão "X quer enviar · nome · tamanho · Recusar/Aceitar" (janela do PC: pilha flutuante; Android: acima das abas) e
  botões no card da transferência. `transfer:ask`, `transfer_asks/accept/decline`.
- Protocolo: capability `file-ask-v1`. Emissor espera o aceite até 10 min (antes 30 s); receptor recusa sozinho em 9 min.
  Peer sem a capability mantém a regra antiga (aceita se "receber automático", senão recusa).
- Queda/reinício com pedido aberto: a linha fica `incoming`, a reoferta reavalia a política e pergunta de novo; aceite dado
  com a conexão caída vale na reoferta.
- Validado (emulador Android 17 + PC): PC→Android com "Sempre perguntar", app reinstalado no meio, aceito após 40 s → SHA igual;
  Android→PC 7,5 KB com limite 100 MB → automático; com "Sempre perguntar" → Recusar → emissor `rejected:declined`
  ("PC recusou o arquivo").

## Confiança e acabamento

### 5. Notificação ao receber no PC — P2 · feito (2026-10-02)
- `notify.rs` (crate `tauri-winrt-notification`): toast **Arquivo recebido de X** com **Abrir / Mostrar na pasta**; toast
  **X quer enviar um arquivo** com **Aceitar / Recusar** (item 4); clique no corpo abre a janela. Texto recebido só com
  "Avisar texto recebido" ligado (padrão desligado); conteúdo sensível nunca aparece no aviso.
- Só com a janela principal sem foco (com foco, a UI já mostra). Ajustes "Notificações do Windows" e "Avisar texto recebido".
- Validado no PC (perfis a/b): toasts gerados; com o "Não incomodar" do Windows ligado eles vão direto para a central de
  notificações; "Mostrar na pasta" abriu o Explorer com o arquivo selecionado e "Aceitar" aceitou o pedido (`completed`).
- Limitações: os botões só funcionam com o ClipSync rodando (sem ativador COM, avisos antigos de um processo anterior não fazem
  nada); em debug o aviso aparece como "Windows PowerShell" (sem AUMID registrado); no build instalado usa o identifier
  `app.clipsync.desktop` — conferir o nome no instalador.

### 6. Iniciar com o Windows — P2
- `tauri-plugin-autostart`, ligado por padrão, abrindo direto no tray. Sem o PC rodando nada sincroniza.

### 7. Permissão de notificação no Android 13+ — P2
- Pedir `POST_NOTIFICATIONS` na primeira transferência. Sem ela o progresso e o botão Cancelar do FGS ficam invisíveis.

### 8. Notificação de "concluído" no Android — P3
- Ao terminar um recebimento: notificação com "Abrir" (o FGS hoje some sem aviso ao concluir).

## Validação

### 9. Aparelho físico — P3
- mDNS na mesma Wi-Fi, QR pela câmera, código digitado, aarch64, throughput real (no emulador o limite é a rede virtual).
- Repetir clipboard nos dois sentidos e 100 MB em cada direção com o build release.
