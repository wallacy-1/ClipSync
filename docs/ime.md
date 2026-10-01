# ClipSync Keyboard (IME opcional do Android)

Estado: **Etapas 1–3b concluídas**: teclado funcional, dois temas, probe (`ime-probe-clipboard.md`) e **ponte real IME → Rust → P2P** com inbox selado (`ime-bridge.md`). Android → Windows automático com o app fechado: provado.

## Regra de produto

O teclado é **opcional**. O ClipSync funciona igual sem ele. O app nunca habilita, seleciona ou troca
o teclado do usuário: os botões levam às telas oficiais do Android (`ACTION_INPUT_METHOD_SETTINGS`,
`InputMethodManager.showInputMethodPicker()`), e o usuário decide lá.

Motivo de existir: a partir do Android 10 só o app com foco **ou o IME padrão** lê o clipboard
(ver `pesquisa-clipboard-background-android.md` §1 e §9). Com o ClipSync Keyboard selecionado, uma
etapa futura poderá detectar cópias com o app em segundo plano. Hoje ele é só um teclado.

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
    ├── ImePrefs.kt                     SharedPreferences (só tema)
    ├── ClipboardBridge.kt              único leitor de clipboard: dedupe por hash → CoreBridge.nativeOnClip ou ImeInbox
    ├── CoreBridge.kt                   JNI fino: nativeOnClip (Rust), setReady, listInbox/removeInbox
    ├── ImeInbox.kt                     fila selada (KeyGuard) para quando o núcleo não está vivo; TTL 1 h
    └── ImeBridge.kt                    chamado do Rust por JNI: status, openSettings, showPicker, setTheme

src-tauri/src/
├── android_jni.rs                      helper with_class() (era privado do keystore; agora compartilhado)
├── ime.rs                              comandos + nativeOnClip + drain do inbox
└── clipboard/dedupe.rs                 anti-loop/dedupe único (watcher, poll, IME, inbox) — testado

src/
├── android/KeyboardView.tsx            Ajustes → Teclado: estado + ações + Aparência
├── android/KeyboardPrompt.tsx          cartão único pós-onboarding ("Agora não" / "Configurar teclado")
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
- A única coisa lida fora do fluxo do teclado é o `ClipboardManager` (`ClipboardBridge`). O texto do clipboard vai só para `clipboard::handle_text` (mesmo pipeline do app) ou para o inbox selado; logs (só debug) têm hash/tamanho, nunca conteúdo.
- Sem permissões novas no manifesto. `BIND_INPUT_METHOD` é a permissão que o **sistema** exige do serviço,
  não uma permissão concedida ao app.

## Comportamento do teclado

- Camadas: letras (QWERTY), `?123` (números/símbolos), `=\<` (símbolos 2). Campos numéricos/telefone abrem em `?123`.
- Shift: toque = próxima letra maiúscula; toque duplo = caps lock; auto-shift em início de frase só se o campo
  pedir `TYPE_TEXT_FLAG_CAP_SENTENCES`.
- Toque longo: acentos/variantes (ex.: `a` → á ã â à ä å; `c` → ç); deslize e solte para escolher.
  Backspace repete ao segurar.
- Enter: executa a ação do campo (`Ir`, `Buscar`, `Enviar`, `Próximo`, `OK`) ou insere `\n` em
  campos multilinha / `IME_FLAG_NO_ENTER_ACTION`.
- 🌐 troca para o próximo teclado (`switchToNextInputMethod`, API 28+; senão abre o seletor).
- Sem tela cheia em paisagem (`onEvaluateFullscreenMode = false`).
- Fora do escopo (por enquanto): autocorreção, sugestões, swipe, emoji, voz, outros layouts.

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
