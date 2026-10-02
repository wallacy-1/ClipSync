#!/bin/bash
# Build Android (debug, APK). Uso: ./build-android.sh [x86_64|aarch64]
export PATH="$HOME/.cargo/bin:$PATH"
export JAVA_HOME="C:\\Program Files\\Android\\Android Studio\\jbr"
# Sem Android Studio: usa o JDK mais novo em "Program Files/Java".
[ -d "$(cygpath -u "$JAVA_HOME")" ] || export JAVA_HOME="$(cygpath -w "$(ls -d "/c/Program Files/Java/"jdk* | sort -V | tail -1)")"
export ANDROID_HOME="C:\\Users\\wwall\\AppData\\Local\\Android\\Sdk"
export NDK_HOME="$ANDROID_HOME\\ndk\\27.1.12297006"
export CARGO_TARGET_DIR="E:/clipsync-build/target"
export TMP="E:/clipsync-build/tmp" TEMP="E:/clipsync-build/tmp"
export GRADLE_USER_HOME="E:/clipsync-build/gradle"
cd "$(dirname "$0")" && npx tauri android build --debug --apk --target "${1:-x86_64}"
