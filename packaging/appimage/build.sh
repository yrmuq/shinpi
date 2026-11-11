#!/bin/sh
# Copyright (C) 2026 yrmuq.
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)"
PACK="$ROOT/packaging/appimage"
APPDIR="$PACK/build/AppDir"
CACHE="$PACK/build/cache"
DIST="$ROOT/dist"
mkdir -p "$CACHE" "$DIST"

TOOL="$CACHE/appimagetool-x86_64.AppImage"
TOOL_URL="https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage"
TOOL_SHA="ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0"
if [ ! -s "$TOOL" ] || ! echo "$TOOL_SHA  $TOOL" | sha256sum -c - >/dev/null 2>&1; then
    curl -fL --retry 3 -o "$TOOL.part" "$TOOL_URL"
    echo "$TOOL_SHA  $TOOL.part" | sha256sum -c -
    mv "$TOOL.part" "$TOOL"
fi
chmod +x "$TOOL"

cd "$ROOT"
cargo build --locked --release
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release/shinpi"
test -x "$BIN"

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib" \
    "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/icons/hicolor/scalable/apps" \
    "$APPDIR/usr/share/doc/shinpi"
cp "$BIN" "$APPDIR/usr/bin/shinpi"
mkdir -p "$APPDIR/usr/share/shinpi/languages"
cp "$ROOT"/languages/*.ini "$APPDIR/usr/share/shinpi/languages/"
cp "$ROOT/LICENSE" "$ROOT/NOTICE" "$ROOT/README.md" "$PACK/BUNDLED.txt" \
    "$ROOT/ui/icons/COPYING" "$ROOT/packaging/70-shinpi.rules" \
    "$APPDIR/usr/share/doc/shinpi/"

# Graphics and display transports stay on the host with its GPU driver.
ldd "$BIN" | awk '/=>/ { print $3 }' | while read -r lib; do
    [ -f "$lib" ] || continue
    base=$(basename "$lib")
    case "$base" in
        libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*|libresolv.so*|libutil.so*|ld-linux*) continue ;;
        libGL.so*|libEGL.so*|libGLdispatch.so*|libGLX.so*|libOpenGL.so*|libGLESv2.so*|libGLX_mesa.so*|libEGL_mesa.so*|libglapi.so*|libdrm.so*|libgbm.so*|libgallium*.so*|libwayland-*.so*|libxkbcommon*.so*) continue ;;
    esac
    cp -L "$lib" "$APPDIR/usr/lib/$base"
done
find "$APPDIR/usr/lib" -type f -printf '%f\n' | sort > "$APPDIR/usr/share/doc/shinpi/LIBS.txt"
mkdir -p "$APPDIR/usr/share/doc/shinpi/system-licenses"
for package in libfontconfig1 libfreetype6 libpng16-16t64 libbrotli1 libexpat1 libbz2-1.0 zlib1g libgcc-s1 libstdc++6; do
    if [ -f "/usr/share/doc/$package/copyright" ]; then
        cp -L "/usr/share/doc/$package/copyright" "$APPDIR/usr/share/doc/shinpi/system-licenses/$package"
    fi
done

cp "$PACK/AppRun" "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun"
cp "$ROOT/packaging/io.github.yrmuq.shinpi.desktop" "$APPDIR/shinpi.desktop"
cp "$APPDIR/shinpi.desktop" "$APPDIR/usr/share/applications/io.github.yrmuq.shinpi.desktop"
cp "$ROOT/assets/icon.svg" "$APPDIR/shinpi.svg"
cp "$APPDIR/shinpi.svg" "$APPDIR/.DirIcon"
cp "$APPDIR/shinpi.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/shinpi.svg"

OUT="$DIST/shinpi-x86_64.AppImage"
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$OUT"
chmod +x "$OUT"
cd "$DIST"
sha256sum shinpi-x86_64.AppImage > shinpi-x86_64.AppImage.sha256
