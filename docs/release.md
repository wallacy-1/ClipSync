# Release: instalador Windows e APK Android

Um comando gera tudo em `release\v<versão>\` (pasta ignorada pelo git):

```powershell
npm run release:windows   # instalador .exe/.msi + latest.json (atualização automática)
npm run release:android   # APK assinado (arm64)
npm run release           # os dois
```

Por baixo é `scripts/release.ps1`; opções extras:

```powershell
.\scripts\release.ps1 windows -Notes "Corrige X e Y"          # texto mostrado no aviso de atualização
.\scripts\release.ps1 android -AndroidTargets aarch64,armv7   # APK também para celulares 32 bits
.\scripts\release.ps1 android -DebugSign                      # APK de TESTE (keystore de debug, não publicar)
.\scripts\release.ps1 windows -Repo usuario/ClipSync          # repo do GitHub, se não houver remote
```

## Chaves (nunca no repositório)

O repositório é público. Chaves e senhas ficam **fora** dele, numa pasta lida pelo script:

```
<pasta acima do repo>\chaves\ClipSync\     (ou a pasta em $env:CLIPSYNC_KEYS_DIR)
  updater.key          chave privada que assina as atualizações do Windows
  updater.password     senha dela (uma linha)
  updater.key.pub      chave pública (a mesma que está em src-tauri/tauri.conf.json → plugins.updater.pubkey)
  android-release.jks  keystore do APK (criado por `npm run release:android-key`)
  android.password     senha do keystore
  SENHAS.md            anotação humana das senhas
```

A chave pública pode ficar no repo; a privada e as senhas **não**. O `.gitignore` bloqueia
`*.key`, `*.password`, `*.jks` e `SENHAS.md` por segurança, mas o lugar delas é a pasta acima.

Faça backup dessa pasta. Perdeu:
- `updater.key` ou a senha → instalações existentes não aceitam mais atualizações (só reinstalando);
- `android-release.jks` ou a senha → o Android recusa instalar o APK novo por cima do antigo (só desinstalando, o que apaga histórico e pareamentos).

### Primeira vez numa máquina nova

1. Copie a pasta `chaves\ClipSync` do backup para o lado do repo (ou aponte `CLIPSYNC_KEYS_DIR`).
2. Só se ainda não existir keystore do Android: `npm run release:android-key` (gera e anota a senha em `SENHAS.md`).
3. Pré-requisitos de build: ver README → Desenvolvimento (Rust, Node, VS Build Tools; Android: SDK, NDK, JDK 17+).

Para gerar uma chave do updater nova (só se nunca houve release, senão quebra as atualizações):

```powershell
npx tauri signer generate --ci -p "<senha>" -w ..\chaves\ClipSync\updater.key
```

e cole o conteúdo de `updater.key.pub` em `plugins.updater.pubkey`.

## Publicar uma versão

1. Suba a versão em **três lugares** (iguais): `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
   O `versionCode` do Android sai sozinho da versão (`1.2.3` → `1002003`).
2. `npm run release`
3. No GitHub: **Releases → Draft a new release**, tag `v<versão>` (ex.: `v1.0.1`), e anexe de `release\v<versão>\`:
   - `ClipSync_<versão>_x64-setup.exe`
   - `ClipSync_<versão>_x64-setup.exe.sig`
   - `latest.json`
   - `ClipSync_<versão>.apk`
   - (opcional) o `.msi`
4. Publique a release (não deixe como pre-release/draft: o endpoint usa `releases/latest`).

As instalações Windows procuram `https://github.com/<repo>/releases/latest/download/latest.json`
15 s após abrir e a cada 6 h; o usuário confirma em Ajustes → Atualização. O pacote só é instalado
se a assinatura bater com a chave pública embutida no app.

O endpoint está em `src-tauri/tauri.conf.json` → `plugins.updater.endpoints`. Se o repositório no
GitHub tiver outro dono/nome, corrija ali **antes** de distribuir a primeira versão (versões já
instaladas só olham para a URL com que foram compiladas).

## Android

O APK não se atualiza sozinho: o usuário baixa o novo `.apk` da release e instala por cima.
Instalar por cima só funciona com o **mesmo keystore**; por isso o script se recusa a assinar
sem o keystore de release (a não ser com `-DebugSign`, para teste).

Instalar num aparelho conectado por USB:

```powershell
adb install -r release\v1.0.0\ClipSync_1.0.0.apk
```
