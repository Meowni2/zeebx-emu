#!/usr/bin/env bash
# Compila o núcleo para o Android e monta a APK.
#
# Tudo o que o Android precisa mora no $HOME, sem sudo: ver `frontends/android/LEIAME.md`.
set -euo pipefail

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAIZ="$(cd "$AQUI/../.." && pwd)"
: "${ANDROID_SDK_HOME:=$HOME/Android/sdk}"
: "${ANDROID_NDK_VERSAO:=27.3.13750724}"
: "${ZEEBX_ANDROID_ABI:=arm64-v8a}"
: "${ZEEBX_ANDROID_PLATFORM:=android-35}"

export ANDROID_HOME="$ANDROID_SDK_HOME"
export ANDROID_NDK_HOME="$ANDROID_SDK_HOME/ndk/$ANDROID_NDK_VERSAO"
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
export ANDROID_NDK="$ANDROID_NDK_HOME"
export JAVA_HOME="${JAVA_HOME:-$HOME/Android/jdk}"
export ZEEBX_ANDROID_ABI ZEEBX_ANDROID_PLATFORM
export CMAKE_TOOLCHAIN_FILE="$AQUI/ndk-toolchain.cmake"
export CMAKE_MAKE_PROGRAM="$(command -v ninja)"

NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
ALVO_RUST="aarch64-linux-android"

# No bionic a pthread e a rt vivem dentro da libc: não existe libpthread.so nem librt.so. O
# `unicorn-engine-sys` pede as duas por nome, sem olhar o sistema, então elas existem aqui como
# arquivos vazios — o ligador acha o nome, não acha símbolo nenhum, e segue.
VAZIAS="$RAIZ/target/android-libs-vazias"
if [ ! -f "$VAZIAS/libpthread.a" ]; then
  mkdir -p "$VAZIAS"
  : > "$VAZIAS/vazio.c"
  "$NDK_BIN/clang" --target="$ALVO_RUST${ZEEBX_ANDROID_PLATFORM#android-}" \
    -c "$VAZIAS/vazio.c" -o "$VAZIAS/vazio.o"
  for nome in pthread rt; do
    "$NDK_BIN/llvm-ar" rcs "$VAZIAS/lib$nome.a" "$VAZIAS/vazio.o"
  done
fi
export RUSTFLAGS="${RUSTFLAGS:-} -L native=$VAZIAS"

cd "$RAIZ"
JNI="$AQUI/apk/app/src/main/jniLibs"
mkdir -p "$JNI"
cargo ndk -t "$ZEEBX_ANDROID_ABI" --platform "${ZEEBX_ANDROID_PLATFORM#android-}" \
  -o "$JNI" build --release -p zeebx-android

# A STL do NDK é uma biblioteca à parte, e o C++ do dynarmic depende dela. Sem este arquivo
# dentro da APK o `dlopen` falha na abertura com "library libc++_shared.so not found" e a
# atividade morre antes de o `android_main` rodar.
SYSROOT="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib"
case "$ZEEBX_ANDROID_ABI" in
  arm64-v8a)   TRIPLA="aarch64-linux-android" ;;
  armeabi-v7a) TRIPLA="arm-linux-androideabi" ;;
  x86_64)      TRIPLA="x86_64-linux-android" ;;
  *) echo "ABI sem tripla conhecida: $ZEEBX_ANDROID_ABI" >&2; exit 1 ;;
esac
cp "$SYSROOT/$TRIPLA/libc++_shared.so" "$JNI/$ZEEBX_ANDROID_ABI/"

echo "== .so =="
ls -lh "$JNI/$ZEEBX_ANDROID_ABI/"

if [ "${1:-}" = "--apk" ]; then
  cd "$AQUI/apk"
  "${GRADLE:-$HOME/Android/gradle/bin/gradle}" --no-daemon assembleDebug
  echo "== APK =="
  ls -lh "$AQUI/apk/app/build/outputs/apk/debug/"
fi
