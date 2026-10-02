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

### 4. Aceitar/recusar arquivo recebido — P2
- Hoje o aceite é sempre automático (`auto_receive`): um dispositivo pareado pode encher os Downloads.
- Fazer: prompt "Aceitar/Recusar" com nome, tamanho e origem; limite de tamanho para aceite automático por dispositivo.

## Confiança e acabamento

### 5. Notificação ao receber no PC — P2
- Toast do Windows para "arquivo recebido" (com Abrir / Mostrar na pasta) e, opcionalmente, "texto recebido".

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
