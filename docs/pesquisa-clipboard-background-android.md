# Pesquisa: detectar cópias no Android sem abrir o ClipSync

Data: 2026-09-18 · Alvo: Android 15+/17 (API 37, emulador Pixel 8a) · targetSdk 36.
Objetivo: `usuário copia → ClipSync detecta → envia ao Windows`, sem abrir o app a cada cópia.
Nada aqui foi implementado; só pesquisa + os probes já feitos (`probes/fgs-clipboard`).

Legenda: **[F]** fato documentado/verificado · **[P]** provado empiricamente neste projeto · **[H]** hipótese/inferência.

## 1. ClipboardManager — regra vigente

- [F] Desde o Android 10: "unless your app is the default input method editor (IME) or is the app that currently has focus, your app cannot access clipboard data" (developer.android.com, *Privacy changes in Android 10*).
- [F] No AOSP (`ClipboardService.clipboardAccessAllowed`), leitura é permitida apenas para: quem tem a permissão de assinatura `READ_CLIPBOARD_IN_BACKGROUND` (sistema/privilegiados), o IME padrão, o UID com foco de janela no display (ou em display de VirtualDevice), janelas `INTERNAL_SYSTEM_WINDOW` com foco, o serviço de Content Capture e o de Autofill do sistema, e o dono privilegiado de um VirtualDevice. **Accessibility Service não tem exceção.**
- [P] Em API 37 com app em background: `hasPrimaryClip()=false`, `getPrimaryClipDescription()=null`, `getPrimaryClip()=null`, listener nunca dispara; sem `SecurityException` — a API responde "vazio". Com foco: leitura no primeiro poll.
- [F] Android 12+: aviso visual do sistema quando um app lê clipboard de outro. Android 13+: limpeza automática (~1 h) e flag `EXTRA_IS_SENSITIVE`.
- [F] Não há nenhuma permissão de runtime que um app comum possa pedir para ler clipboard em background.

## 2. Foreground Service

- [F] targetSdk 34+: tipo obrigatório no manifesto e no `startForeground()`, permissão `FOREGROUND_SERVICE` + a do tipo; `POST_NOTIFICATIONS` em runtime (API 33+). Tipos relevantes: `dataSync` (limite de **6 h por 24 h** no Android 15, timer zera ao voltar ao foreground; não pode nascer de `BOOT_COMPLETED`), `connectedDevice`, `specialUse` (revisão no Play Console). Declaração de tipos também no Play Console.
- [F] FGS não pode ser iniciado a partir do background (exceções limitadas).
- [F] Android 15: rede fora de um ciclo de vida válido (processo *cached*) lança `IOException`/`UnknownHostException`. Um FGS mantém o processo fora do estado *cached* → **mantém rede e conexão P2P vivas**.
- [P] FGS `dataSync` ativo, `procState=FGS`, 200 s em background: clipboard continua invisível e listener não dispara. **Conclusão B** (`probes/fgs-clipboard/README.md`).
- Resumo: FGS resolve *receber* em background (rede), não *detectar* cópias.

## 3–4. APIs cross-device / CompanionDeviceManager

- [F] `CompanionDeviceManager` (API 37, verificado no `android.jar`): associação, presença (`startObservingDevicePresence`), transferência de dados do **sistema** (`startSystemDataTransfer` = permissões/consentimento), `attachSystemDataTransport` (túnel de bytes para o sistema usar), notificação de acesso. **Nenhuma API de clipboard.** Não altera a regra de foco.
- [F] *Cross device SDK* (Google): ainda Developer Preview, só via Play Services beta, só telefone/tablet, dois dispositivos, "não para produção". Não cobre Windows.
- [F] *Universal Clipboard* (Android 17): `UniversalClipboardManager` em `android.companion.datatransfer.continuity` apareceu em builds canary/beta como **serviço de sistema**, operado pelo Pixel System Service com `READ_CLIPBOARD_IN_BACKGROUND` e Play Services; **não está no SDK público API 37** (verificado: ausente do `android.jar`). Escopo especulado: dispositivos Google/Android PC. Não é usável por apps de terceiros.
- [F] Microsoft Phone Link "cross-device copy and paste" só funciona em Surface Duo, Samsung One UI 2.1+, HONOR/OPPO/vivo/ASUS/Xiaomi selecionados — o componente *Link to Windows* é pré-instalado/privilegiado pelo OEM. Não replicável por app comum.
- [F] Nearby Connections / Nearby Share: transporte e compartilhamento iniciados pelo usuário; não observam clipboard.

## 5–6. Eventos públicos, ContentProvider, notificações

- [F] Único evento público de clipboard: `OnPrimaryClipChangedListener`, sujeito à mesma regra de foco (Android 10+). Não existe broadcast, `ContentObserver`, ContentProvider ou notificação do sistema que exponha "houve uma cópia" a apps sem foco. O aviso "app X colou…" e o overlay de "Copiado" do Android 13 são UI do sistema, não eventos consumíveis (não passam pelo `NotificationListenerService`).
- [F] ContentProvider só participa como *destino* de `ClipData` (URIs de conteúdo); não gera eventos de cópia.

## 7. "Clipboards remotos" existentes

- Todos os casos oficiais (Phone Link, Universal Clipboard, Samsung/Chrome OS) rodam com privilégio de sistema. Apps comuns legítimos (KDE Connect, XClipper, CrossPaste, "Clipboard - Copy Paste & Notes") usam: (a) ação manual — botão "Send clipboard" na notificação persistente e **Quick Settings Tile** (KDE Connect, Android 14+); (b) Accessibility Service para *perceber* o gesto de copiar e então ganhar foco brevemente para ler; (c) KDE Connect: leitura do próprio logcat (`READ_LOGS`, concedida por ADB) para saber quando foi negado e abrir uma janela invisível — gambiarra explicitamente marcada como tal.

## 8. Android 15/16/17

- [F] 15: rede cortada em *cached*; FGS `dataSync` 6 h/24 h; restrições de start em `BOOT_COMPLETED`.
- [F] 16/17: nenhuma mudança pública na regra de acesso ao clipboard encontrada; 17 traz continuidade de clipboard **só para o sistema** (acima). Nada novo para apps de terceiros.

## 9. IME (teclado) como alternativa

- Como resolveria: [F] o **IME padrão** é a única exceção de app de terceiro na regra de acesso; um `InputMethodService` selecionado como teclado padrão recebe `OnPrimaryClipChangedListener` e lê `getPrimaryClip()` a qualquer momento — é assim que o Gboard mantém histórico.
- Limitações: [F] só o teclado **padrão** ganha isso; o usuário precisa instalar, ativar em *Configurações → Teclados* e **trocar** seu teclado (substituindo Gboard/Samsung). O IME precisa ser um teclado completo e utilizável (layouts, idiomas, autocorreção, emoji, voz) — escopo de produto inteiro; não dá para "embutir" outro teclado. Um IME não selecionado não roda.
- UX: troca de teclado é a maior fricção possível no Android; qualquer defeito do nosso teclado vira defeito do sistema para o usuário.
- Distribuição: [F] o Play aceita IMEs; a política exige transparência sobre coleta de texto digitado (o teclado vê tudo que é digitado) — privacidade sensível; revisões de "keyboard apps" são mais rigorosas (formulário de dados, política de privacidade).
- Veredito: tecnicamente possível e oficial, mas inviável como MVP e desproporcional ao problema.

## 10. Accessibility Service

- [F] Não dá acesso ao clipboard (sem exceção no `ClipboardService`). O que apps fazem: detectar o evento de "Copiar" (ex.: `TYPE_VIEW_TEXT_SELECTION_CHANGED`, cliques em menus de seleção) e então **ganhar foco por alguns ms** (overlay/activity invisível) para ler — ou ler o texto selecionado direto da árvore de acessibilidade. [H] Isso é frágil: depende de OEM/teclado/app de origem, do comportamento de overlays em Android 14+ e da política de start em background.
- Permissões: `BIND_ACCESSIBILITY_SERVICE`, ativação manual pelo usuário em *Acessibilidade* com aviso de "controle total do dispositivo"; Android 13+ bloqueia por padrão apps sideloaded ("Restricted settings").
- [F] Play: uso não-acessibilidade só para fins "estreitos e claramente entendidos", com **divulgação proeminente + consentimento**, formulário de declaração obrigatório (targetSdk 31+); risco real de rejeição/remoção. Privacidade: o serviço enxerga o conteúdo de todas as telas.
- Veredito: não resolve o problema de forma direta, é o caminho mais invasivo e com maior risco de quebra e de política. Não recomendado.

## 11. Mecanismos sem ler o clipboard em background (todos oficiais)

- **`ACTION_PROCESS_TEXT`** (API 23+): o ClipSync aparece na **barra de seleção de texto** de qualquer app; o usuário seleciona → toca "ClipSync" → a activity recebe `EXTRA_PROCESS_TEXT` sem passar pelo clipboard nem precisar de foco prévio. 1 toque a mais que "Copiar", em vez de copiar+abrir o app.
- **Share sheet** (`ACTION_SEND` text/plain) + **Direct Share/shortcuts** ("Enviar para Estação-Dev"): idem, via "Compartilhar".
- **Quick Settings Tile** (`TileService`): "Enviar clipboard" → `startActivityAndCollapse` abre activity transparente com foco → lê → envia → fecha. É o que o KDE Connect faz no Android 14+.
- **Ação na notificação persistente** (do FGS) "Enviar clipboard": mesmo princípio.
- [F] Todos exigem **uma interação do usuário por item**; nenhum é automático.

## 12. Matriz comparativa

| Alternativa | Tec. possível | Oficial | Background | API 37 | Interação/usuário | Accessibility | IME | FGS | UX | Play Store | Risco futuro |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Polling/listener com app em foreground (atual) | sim | sim | **não** | sim [P] | abrir o app | não | não | não | fraca (abrir app) | ok | baixo |
| FGS + polling | sim | sim | **não** (só rede) [P] | sim | — | não | não | sim | notificação fixa | ok (declarar tipo) | médio (limites de FGS) |
| Quick Settings Tile "Enviar clipboard" | sim | sim | sim* | sim [F] | 1 toque por item | não | não | não | boa (1 toque, sem abrir app) | ok | baixo |
| Notificação persistente com ação "Enviar" | sim | sim | sim* | sim [F] | 1 toque por item | não | não | sim | ok (notificação fixa) | ok | baixo |
| `ACTION_PROCESS_TEXT` na barra de seleção | sim | sim | sim* | sim [F] | 1 toque (substitui "Copiar") | não | não | não | **boa** (integrado ao gesto) | ok | baixo |
| Share sheet / Direct Share | sim | sim | sim* | sim [F] | 2 toques | não | não | não | ok | ok | baixo |
| IME próprio (teclado padrão) | sim | sim | **sim** [F] | sim | trocar teclado (1x) | não | **sim** | não | **péssima** (substitui teclado) | rigorosa (dados de digitação) | médio |
| Accessibility + foco temporário | parcial [H] | cinza | parcial [H] | ? | ativar serviço (1x) | **sim** | não | não | ruim (aviso de controle total) | **alto risco** | **alto** |
| `READ_LOGS`/janela invisível (KDE) | sim (ADB) | não | parcial | ? | concessão via ADB | não | não | não | inaceitável | não | alto |
| CompanionDeviceManager | sim | sim | n/a (sem clipboard) | sim | pareamento | não | não | não | — | ok | baixo |
| Cross device SDK | preview | preview | n/a | ? | — | não | não | não | — | não (preview) | alto |
| Universal Clipboard (Android 17) | não p/ terceiros | sistema | sim | só sistema | — | — | — | — | — | — | — |
| `READ_CLIPBOARD_IN_BACKGROUND` / OEM (Phone Link) | só privilegiado | sistema | sim | sistema | — | — | — | — | — | — | — |

\* "sim*" = funciona com o ClipSync fechado/em background porque o **sistema** entrega o texto ao app (ou o traz ao foco) no momento do toque; não é monitoramento contínuo.

## 13. Conclusão e recomendação (sem implementar)

**Fato central:** não existe caminho oficial para um app comum observar o clipboard em background no Android 10–17. As únicas exceções são IME padrão e componentes de sistema/privilegiados (Phone Link, Universal Clipboard). FGS, CompanionDeviceManager, Cross device SDK e Accessibility não mudam isso.

**Recomendação arquitetural (fatos, sem código):**

1. **Manter a regra de produto realista:** no Android, "copiar → sincronizar" automático só ocorre com o ClipSync em foreground (já funciona, ≤ 2 s). Documentar como limitação de plataforma, não bug.
2. **Adicionar as entradas oficiais de 1 toque, integradas ao gesto do usuário**, em ordem de valor:
   - `ACTION_PROCESS_TEXT` ("ClipSync" na barra de seleção — o usuário nem precisa copiar);
   - Quick Settings Tile "Enviar clipboard";
   - ação "Enviar clipboard" na notificação do FGS;
   - Share target / Direct Share por dispositivo.
   Todos são APIs públicas estáveis desde API 23–24, sem permissões especiais, com UX aceitável e zero risco de política.
3. **Foreground Service (`dataSync` ou `connectedDevice`) apenas para receber**: mantém a conexão P2P e a fila funcionando com o app em background (provado que sem ele o Android 15 fecha os sockets). Precisa de notificação persistente e respeitar 6 h/24 h no `dataSync` — avaliar `connectedDevice` (sem esse limite) já que o caso é conexão com dispositivo pareado. Sem `specialUse` para não depender de revisão manual.
4. **Não usar** Accessibility (não lê clipboard, política e privacidade pesadas, frágil) nem IME (troca o teclado do usuário; produto inteiro à parte). Só reconsiderar IME se um dia virar objetivo de produto ter um teclado próprio.
5. **Acompanhar** Universal Clipboard do Android 17: se virar API pública/companion para terceiros, seria a solução definitiva; hoje não é.

## Fontes

- Android 10 privacy changes (clipboard): https://developer.android.com/about/versions/10/privacy/changes
- Copy & paste (ClipData, Android 12/13): https://developer.android.com/develop/ui/views/touch-and-input/copy-paste
- Secure clipboard handling: https://developer.android.com/privacy-and-security/risks/secure-clipboard-handling
- AOSP `ClipboardService.java` (regra `clipboardAccessAllowed`): https://android.googlesource.com/platform/frameworks/base/+/master/services/core/java/com/android/server/clipboard/ClipboardService.java
- FGS types (dataSync/specialUse/connectedDevice): https://developer.android.com/develop/background-work/services/fgs/service-types
- Android 15 behavior changes (rede em cached, 6 h dataSync): https://developer.android.com/about/versions/15/behavior-changes-all · https://developer.android.com/about/versions/15/behavior-changes-15
- `ACTION_PROCESS_TEXT`: https://developer.android.com/reference/android/content/Intent#ACTION_PROCESS_TEXT
- CompanionDeviceManager: https://developer.android.com/reference/android/companion/CompanionDeviceManager (métodos conferidos no `android.jar` API 37 local)
- Cross device SDK (preview): https://developer.android.com/guide/topics/connectivity/cross-device-sdk/get-started
- Android 17 Universal Clipboard (canary, sistema): https://www.androidauthority.com/android-17-universal-clipboard-rumor-3618614/
- Play — política de AccessibilityService: https://support.google.com/googleplay/android-developer/answer/10964491
- KDE Connect (tile/notificação "Send clipboard", workaround READ_LOGS): https://userbase.kde.org/KDEConnect · https://bugs.kde.org/show_bug.cgi?id=446366
- XClipper / CrossPaste (uso de Accessibility): https://play.google.com/store/apps/details?id=com.kpstv.xclipper · https://crosspaste.com/en/mobile-tutorial/android-accessibility
- Phone Link — dispositivos suportados: https://support.microsoft.com/en-us/topic/seamlessly-transfer-content-between-your-devices-8a0ead3c-2f15-1338-66ca-70cf4ae81fcb
- Probe deste projeto: `probes/fgs-clipboard/README.md`
