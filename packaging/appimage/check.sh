#!/bin/sh
set -eu
export APPIMAGE_EXTRACT_AND_RUN=1
ARCH=$(uname -m)
dist/shinpi-$ARCH.AppImage --help
dist/shinpi-$ARCH.AppImage --once | target/debug/shinpi-tools check-readings
dist/shinpi-$ARCH.AppImage --print-udev-rules | cmp - packaging/70-shinpi.rules
(cd dist && sha256sum -c shinpi-$ARCH.AppImage.sha256)
test -s "dist/shinpi-$ARCH.AppImage.zsync"
env -u APPIMAGE_EXTRACT_AND_RUN dist/shinpi-$ARCH.AppImage --appimage-updateinformation | \
    grep -Fx "gh-releases-zsync|yrmuq|shinpi|latest|shinpi-$ARCH.AppImage.zsync"
target/debug/shinpi-tools check-zsync "dist/shinpi-$ARCH.AppImage"
