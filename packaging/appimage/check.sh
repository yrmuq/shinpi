#!/bin/sh
set -eu
export APPIMAGE_EXTRACT_AND_RUN=1
dist/shinpi-x86_64.AppImage --help
dist/shinpi-x86_64.AppImage --once | python3 -c 'import json,sys; data=json.load(sys.stdin); assert isinstance(data["devices"], list) and isinstance(data["warnings"], list)'
dist/shinpi-x86_64.AppImage --print-udev-rules | cmp - packaging/70-shinpi.rules
(cd dist && sha256sum -c shinpi-x86_64.AppImage.sha256)
