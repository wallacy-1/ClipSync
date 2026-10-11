# ClipSync Keyboard (IME opcional do Android)

Estado: **Etapas 1–3b concluídas**: teclado funcional, dois temas, probe (`ime-probe-clipboard.md`) e **ponte real IME → Rust → P2P** com inbox selado (`ime-bridge.md`). Android → Windows automático com o app fechado: provado.

## Regra de produto

O teclado é **opcional**. O ClipSync funciona igual sem ele. O app nunca habilita, seleciona ou troca
o teclado do usuário: os botões levam às telas oficiais do Android (`ACTION_INPUT_METHOD_SETTINGS`,
`InputMethodManager.showInputMethodPicker()`), e o usuário decide lá.

Motivo de existir: a partir do Android 10 só o app com foco **ou o IME padrão** lê o clipboard
(ver `pesquisa-clipboard-background-android.md` §1 e §9). Com o ClipSync Keyboard selecionado, as
cópias feitas com o app em segundo plano são detectadas e entregues ao núcleo (`ime-bridge.md`).

## Onde está

```
src-tauri/gen/android/app/src/main/
├── AndroidManifest.xml                 <service .ime.ClipSyncImeService> (BIND_INPUT_METHOD)
├── res/xml/method.xml                  subtypes pt-BR / en-US
├── res/font/figtree.ttf                tema ClipSync (OFL, app/OFL-Figtree.txt)
└── java/app/clipsync/desktop/ime/
    ├── ClipSyncImeService.kt           InputMethodService: commit, backspace, enter, shift, camadas
    ├── KeyboardView.kt                 View própria (Canvas): desenha layout × tema, toque, long-press
    ├── KeyboardLayout.kt               dados puros: camadas letters / symbols / symbols2, acentos
    ├── KeyboardTheme.kt                interface + DefaultTheme + ClipSyncTheme + KeyboardThemes.all
    ├── ImePrefs.kt                     SharedPreferences (tema + emojis recentes)
    ├── ImePanels.kt                    painéis por cima das teclas: emoji e histórico do ClipSync
    ├── EmojiData.kt                    emojis por categoria
    ├── Suggester.kt                    sugestões/autocorreção pelo corretor ortográfico do sistema
    ├── ClipboardBridge.kt              único leitor de clipboard: texto (dedupe por hash) e imagem → núcleo ou ImeInbox
    ├── CoreBridge.kt                   JNI fino: nativeOnClip, nativeOnImage, nativeHistory, setReady, inbox
    ├── ImeInbox.kt                     fila para quando o núcleo não está vivo; TTL 1 h. Texto selado (KeyGuard),
    │                                   imagem em PNG (até 5)
    └── ImeBridge.kt                    chamado do Rust por JNI: status, openSettings, showPicker, setTheme

src-tauri/src/
├── android_jni.rs                      helper with_class() (era privado do keystore; agora compartilhado)
├── ime.rs                              comandos + nativeOnClip/nativeOnImage/nativeHistory + drain do inbox
├── clipboard/image.rs                  ingest_inbox_png: imagem do inbox entra no histórico e segue para os pareados
└── clipboard/dedupe.rs                 anti-loop/dedupe único (watcher, poll, IME, inbox) — testado

src/
├── android/KeyboardView.tsx            Ajustes → Teclado: estado + ações + Aparência
├── android/PermissionsSetup.tsx        permissões no onboarding (notificações, teclado, manter conectado)
└── store/app.ts                        ime, refreshIme, setImeTheme, openImeSettings, showImePicker
```

## Estados detectados (APIs públicas)

| Estado | Como | UI |
|---|---|---|
| Desativado | `InputMethodManager.enabledInputMethodList` não contém o componente | ○ Desativado · [Ativar ClipSync Keyboard] → Configurações do Android |
| Instalado, não selecionado | habilitado, mas `Settings.Secure.DEFAULT_INPUT_METHOD` ≠ componente | ⚠ · [Selecionar ClipSync Keyboard] → seletor do sistema |
| Ativo | `DEFAULT_INPUT_METHOD` == componente | ✓ Ativo · [Configurações de teclado] |

A tela re-lê o estado em `visibilitychange` (volta das Configurações) e 1,5 s após cada ação.

## Temas

`KeyboardTheme` é só um objeto de valores (paleta claro/escuro, raio, gaps, fonte). `KeyboardView`
desenha qualquer `KeyboardLayer` com qualquer tema — um renderer, zero duplicação de layout.
Novo tema = implementar a interface e adicionar em `KeyboardThemes.all`; a lista chega ao React por
`ime_status().themes`, então a tela de Aparência não sabe quantos temas existem.

- `default` (padrão para todo usuário novo): cinzas neutros, segue `uiMode` claro/escuro. "É só um teclado normal."
- `clipsync`: tokens de `src/styles/tokens.css` (surface `#ebddc5`, texto `#201e1d`, accent `#c67139`
  no Enter, accent-2 `#7a8a5e` no caps lock, raio 10 dp, Figtree 600). Sem logotipo em tecla.

Tema fica em `SharedPreferences` (não no SQLite do núcleo) porque o IME precisa lê-lo mesmo com o app
ClipSync fechado. O serviço observa a preferência e re-tematiza sem reiniciar.

## Privacidade (o que o teclado NÃO faz)

- Não registra, persiste, transmite ou analisa texto digitado. Nenhum `Log` recebe texto.
- Campos de senha (`TYPE_TEXT_VARIATION_PASSWORD`, `VISIBLE_PASSWORD`, `WEB_PASSWORD`,
  `TYPE_NUMBER_VARIATION_PASSWORD`): sem auto-capitalização e sem nenhum processamento além do commit.
- A única coisa lida fora do fluxo do teclado é o `ClipboardManager` (`ClipboardBridge`). O texto do clipboard vai só para `clipboard::handle_text` (mesmo pipeline do app) ou para o inbox selado; imagem vai para `image::poll` (mesmo pipeline do polling) ou, com o núcleo morto, vira PNG no inbox. Logs (só debug) têm hash/tamanho, nunca conteúdo. O chip de colar da barra de cima lê o mesmo clipboard só para desenhá-lo na tela.
- Sugestões: a palavra em digitação vai para a sessão do corretor ortográfico do **sistema** (`SpellCheckerSession`,
  o mesmo que sublinha erros nos apps); nada é guardado nem aprendido. Desligado em senha, URL, e-mail e campos com
  `TYPE_TEXT_FLAG_NO_SUGGESTIONS`.
- Guardado em disco pelo teclado: só o tema e os últimos emojis usados (aba "recentes").
- Sem permissões novas no manifesto. `BIND_INPUT_METHOD` é a permissão que o **sistema** exige do serviço,
  não uma permissão concedida ao app.

## Comportamento do teclado

- Camadas: letras (QWERTY), `?123` (números/símbolos), `=\<` (símbolos 2). Campos de número/data abrem o teclado
  numérico grande (3 colunas de dígitos, como no Gboard); telefone abre o mesmo com `* # +`.
- Em paisagem as teclas e a barra encolhem (~70% da altura) para sobrar espaço ao app.
- Shift: toque = próxima letra maiúscula; toque duplo = caps lock; auto-shift quando o campo pede
  (`CAP_SENTENCES`, `CAP_WORDS` ou `CAP_CHARACTERS`).
- Toque longo: acentos/variantes (ex.: `a` → á ã â à ä å; `c` → ç) acima da tecla; deslize e solte para escolher.
  Fileira de cima mostra o número no canto; segurar e soltar digita o número.
- Balão com a letra acima da tecla ao tocar, que fica ~80 ms depois de soltar (como no Gboard). A view tem uma
  faixa transparente no topo para isso; `onComputeInsets` a exclui do teclado (toques ali vão para o app).
- Backspace repete ao segurar; depois de ~1 s passa a apagar palavra por palavra.
- Deslizar na barra de espaço move o cursor. A barra mostra o idioma do subtipo atual.
- Espaço duplo depois de letra/número vira `. ` (nunca em campo de senha). Em `?123` aberto a partir das
  letras, o espaço volta para as letras.
- Enter: executa a ação do campo (ícones de ir/buscar/enviar/próximo/OK; `actionLabel` próprio vira texto) ou
  insere `\n` em campos multilinha / `IME_FLAG_NO_ENTER_ACTION`.
- 🌐 troca para o próximo teclado (`switchToNextInputMethod`, API 28+; senão abre o seletor).
- Barra de cima, parada: emoji, histórico, botão que abre o ClipSync e, quando o clipboard tem algo copiado há
  menos de 10 min, um chip que cola com um toque (some depois de colado). Imagem aparece com miniatura e é colada
  por `commitContent` (cópia no cache servida pelo FileProvider) só em campos que aceitam imagem; nos outros a barra
  avisa. Clip sensível (`EXTRA_IS_SENSITIVE`) aparece como `••••••••`.
- Barra de cima, digitando: três sugestões do corretor ortográfico do sistema (sem sugestões com espaço, nem muito
  diferentes do digitado). O corretor pode não ter o idioma (o do Gboard só tem os idiomas baixados nele) e aí
  aceita tudo ou responde em outro idioma: cada sessão testa uma palavra comum ("você"/"the") e uma inventada, e
  sem passar no teste não há sugestões (o teste se repete enquanto a pessoa digita, porque o dicionário carrega
  sob demanda).
- Autocorreção (no espaço, em negrito no meio) só no que é quase certo: acento ("voce" → "você") ou duas letras
  vizinhas trocadas ("qeu" → "que"); nunca para nome próprio digitado em minúscula. Backspace logo em seguida
  desfaz e a palavra é mantida. Só em texto comum (não em nome, endereço, etc.).
- Emoji: painel no lugar das teclas, com abas por categoria e "recentes"; ABC volta.
- Histórico: os 30 itens mais recentes do ClipSync (texto e imagem); o toque cola. Com o núcleo fora (processo
  criado só para o teclado), mostra o que o teclado guardou no inbox nesse meio-tempo e um aviso para abrir o app.
- Imagem copiada: logo depois da cópia a permissão de leitura do clipboard pode ainda não valer; a leitura tenta de
  novo algumas vezes (250 ms) e desiste se o clipboard já mudou. No emulador, desligar o eco de clipboard com o PC
  (`clipboardSharing=false`), senão a imagem vira texto em ~10 ms.
- Sem tela cheia em paisagem (`onEvaluateFullscreenMode = false`).
- Fora do escopo (por enquanto): digitação por deslize, voz, outros layouts, sugestões de preenchimento automático.

## Validação manual (emulador)

```bash
adb shell ime list -a -s                      # deve listar app.clipsync.desktop/.ime.ClipSyncImeService
adb shell ime enable app.clipsync.desktop/.ime.ClipSyncImeService   # equivale ao toggle nas Configurações
adb shell ime set app.clipsync.desktop/.ime.ClipSyncImeService      # equivale ao seletor
adb shell settings get secure default_input_method
adb shell ime disable app.clipsync.desktop/.ime.ClipSyncImeService
```

Os comandos `ime enable/set` são só atalho de teste; o app não os executa (não há como, sem shell).

## Próximas etapas

- Restante das etapas 4–6: perda de Wi-Fi/reconexão, Windows offline → online com item vindo do IME, aparelho físico (mDNS).
- File Transfer P2P: passo separado, só após aprovação da 3b.
