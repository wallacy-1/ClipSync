<p align="right"><a href="README.md">Read in English</a></p>

# ClipSync

O ClipSync sincroniza a área de transferência e envia arquivos entre Windows e Android pela sua rede local.
Copia no PC e cola no celular, ou o contrário. Texto, links, imagens e arquivos de qualquer tamanho vão
direto de um aparelho para o outro pelo Wi-Fi, com criptografia de ponta a ponta, sem servidor, sem conta
e sem nuvem. Os dispositivos são pareados uma vez por QR code, e cada um mantém um histórico de clipboard
compartilhado e pesquisável.

## Prints

**Windows**

<p>
  <img src="docs/screenshots/pt/win-history.jpg" height="300" alt="Histórico no Windows" />
  <img src="docs/screenshots/pt/win-transfers.jpg" height="300" alt="Transferências no Windows" />
  <img src="docs/screenshots/pt/win-tray.jpg" height="300" alt="Painel da bandeja no Windows" />
</p>

**Android**

<p>
  <img src="docs/screenshots/pt/android-home.jpg" height="420" alt="Início no Android" />
  <img src="docs/screenshots/pt/android-history.jpg" height="420" alt="Histórico no Android" />
  <img src="docs/screenshots/pt/android-send.jpg" height="420" alt="Enviar no Android" />
</p>

[Mais imagens aqui](docs/screenshots/pt)

## Como rodar

```bash
npm install
npm run tauri dev           # Windows
npm run tauri android dev   # Android
```

## Requisitos

- Node 20+
- Rust (rustup)
- Windows: Visual Studio Build Tools com "Desktop development with C++"
- Android: Android SDK + NDK, JDK 17+, `ANDROID_HOME` e `NDK_HOME` definidos, e os targets do Rust:

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```
