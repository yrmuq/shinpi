#!/bin/sh
set -eu
# Local installation still verifies both signed metadata and application commits.
flatpak remote-add --user --if-not-exists --gpg-import=flatpak-repo/shinpi.gpg shinpi-ci "file://$PWD/flatpak-repo"
if ! flatpak info --user io.github.yrmuq.shinpi >/dev/null 2>&1; then
    flatpak install --user --noninteractive shinpi-ci io.github.yrmuq.shinpi//stable
fi
flatpak update --user --noninteractive io.github.yrmuq.shinpi
flatpak run io.github.yrmuq.shinpi --help
flatpak run io.github.yrmuq.shinpi --once | python3 -c 'import json,sys; d=json.load(sys.stdin); assert isinstance(d["devices"],list) and isinstance(d["warnings"],list)'
flatpak run io.github.yrmuq.shinpi --print-udev-rules | cmp - packaging/70-shinpi.rules
if [ -f "$RUNNER_TEMP/previous-settings.json" ]; then
    cmp "$RUNNER_TEMP/previous-settings.json" "$HOME/.var/app/io.github.yrmuq.shinpi/config/shinpi/config.json"
fi
ostree --repo=flatpak-repo remote delete --if-exists verify
ostree --repo=flatpak-repo remote add --gpg-import=flatpak-repo/shinpi.gpg verify "file://$PWD/flatpak-repo"
for ARCH in x86_64; do
    ostree --repo=flatpak-repo show --gpg-verify-remote=verify "app/io.github.yrmuq.shinpi/$ARCH/stable"
done
