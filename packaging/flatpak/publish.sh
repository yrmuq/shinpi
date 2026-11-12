#!/bin/sh
set -eu
test -n "${FLATPAK_GPG_ID:?}"
flatpak build-update-repo --gpg-sign="$FLATPAK_GPG_ID" flatpak-repo
gpg --export "$FLATPAK_GPG_ID" > flatpak-repo/shinpi.gpg
target/debug/shinpi-tools flatpakref
(cd dist && sha256sum shinpi.flatpakref > shinpi.flatpakref.sha256)
