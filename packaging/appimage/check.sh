#!/bin/sh
set -eu
export APPIMAGE_EXTRACT_AND_RUN=1
ARCH=$(uname -m)
dist/shinpi-$ARCH.AppImage --help
dist/shinpi-$ARCH.AppImage --once | python3 -c 'import json,sys; data=json.load(sys.stdin); assert isinstance(data["devices"], list) and isinstance(data["warnings"], list)'
dist/shinpi-$ARCH.AppImage --print-udev-rules | cmp - packaging/70-shinpi.rules
(cd dist && sha256sum -c shinpi-$ARCH.AppImage.sha256)
test -s "dist/shinpi-$ARCH.AppImage.zsync"
env -u APPIMAGE_EXTRACT_AND_RUN dist/shinpi-$ARCH.AppImage --appimage-updateinformation | \
    grep -Fx "gh-releases-zsync|yrmuq|shinpi|latest|shinpi-$ARCH.AppImage.zsync"
python3 packaging/appimage/check_zsync.py "dist/shinpi-$ARCH.AppImage"
