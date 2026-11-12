#!/bin/sh
set -eu
ARCH=${1:?architecture required}
CHANNEL=${2:?channel required}
case "$ARCH" in x86_64) ;; *) exit 1 ;; esac
case "$CHANNEL" in stable|test|prerelease) ;; *) exit 1 ;; esac
# Vendoring keeps the SDK build offline and tied to Cargo.lock.
mkdir -p cargo dist
if [ ! -d vendor ]; then
    cargo vendor --locked vendor | sed 's@directory = ".*"@directory = "/run/build/shinpi/vendor"@' > cargo/config.toml
fi
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user --noninteractive --arch="$ARCH" flathub \
    org.freedesktop.Sdk//25.08 org.freedesktop.Platform//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08
SIGN=
if [ "$CHANNEL" != test ]; then
    test -n "${FLATPAK_GPG_ID:-}"
    SIGN="--gpg-sign=$FLATPAK_GPG_ID"
fi
flatpak-builder --user --arch="$ARCH" --force-clean --disable-rofiles-fuse \
    --default-branch="$CHANNEL" --repo=flatpak-repo $SIGN \
    "flatpak-build/$ARCH" packaging/flatpak/io.github.yrmuq.shinpi.json
set --
if [ "$CHANNEL" = stable ]; then
    set -- --repo-url=https://yrmuq.github.io/shinpi/ --gpg-keys=dist/shinpi.gpg
elif [ "$CHANNEL" = prerelease ]; then
    set -- --gpg-keys=dist/shinpi.gpg
fi
flatpak build-bundle "$@" flatpak-repo "dist/shinpi-$ARCH.flatpak" io.github.yrmuq.shinpi "$CHANNEL" --arch="$ARCH"
(cd dist && sha256sum "shinpi-$ARCH.flatpak" > "shinpi-$ARCH.flatpak.sha256")
# --run uses a separate parser and rejects --user.
dbus-run-session -- flatpak-builder --arch="$ARCH" --run "flatpak-build/$ARCH" \
    packaging/flatpak/io.github.yrmuq.shinpi.json shinpi --help
rm -rf "flatpak-build/$ARCH"
