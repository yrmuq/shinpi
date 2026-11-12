#!/bin/sh
set -eu
CHANNEL=${1:?channel required}
sh packaging/appimage/build.sh
sh packaging/appimage/check.sh
sh packaging/flatpak/build.sh x86_64 "$CHANNEL"
sh packaging/flatpak/build.sh aarch64 "$CHANNEL"
