#!/bin/bash
# Ambiente de build: target em E: (C: sem espaço). Uso: ./build.sh <cargo args>
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="E:/clipsync-build/target"
export TMP="E:/clipsync-build/tmp" TEMP="E:/clipsync-build/tmp"
cd "$(dirname "$0")/src-tauri" && cargo "$@"
