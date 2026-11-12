#!/bin/sh
set -eu
test -n "${FLATPAK_GPG_KEY:?FLATPAK_GPG_KEY must contain an armored private signing key}"
export GNUPGHOME="$RUNNER_TEMP/shinpi-gnupg"
mkdir -m 700 -p "$GNUPGHOME"
printf '%s' "$FLATPAK_GPG_KEY" | gpg --batch --import
ID=$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1 == "fpr" {print $10; exit}')
test -n "$ID"
printf 'signing preflight' | gpg --batch --yes --local-user "$ID" --detach-sign > "$RUNNER_TEMP/signature"
printf 'GNUPGHOME=%s\nFLATPAK_GPG_ID=%s\n' "$GNUPGHOME" "$ID" >> "$GITHUB_ENV"
gpg --export "$ID" > "$RUNNER_TEMP/shinpi.gpg"
mkdir -p dist
cp "$RUNNER_TEMP/shinpi.gpg" dist/shinpi.gpg
(cd dist && sha256sum shinpi.gpg > shinpi.gpg.sha256)
if [ "${1:-stable}" = prerelease ]; then exit 0; fi
# Keep signed history so installed applications can update without replacing their remote.
STATUS=$(curl -L --retry 3 -o "$RUNNER_TEMP/previous.gpg" -w '%{http_code}' https://yrmuq.github.io/shinpi/shinpi.gpg)
case "$STATUS" in
    200)
        cmp "$RUNNER_TEMP/shinpi.gpg" "$RUNNER_TEMP/previous.gpg"
        ostree --repo=flatpak-repo init --mode=archive
        ostree --repo=flatpak-repo remote add --gpg-import="$RUNNER_TEMP/shinpi.gpg" previous https://yrmuq.github.io/shinpi/
        ostree --repo=flatpak-repo pull --mirror previous
        flatpak build-update-repo --gpg-sign="$ID" flatpak-repo
        flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
        flatpak remote-add --user --if-not-exists --gpg-import="$RUNNER_TEMP/shinpi.gpg" shinpi-ci "file://$PWD/flatpak-repo"
        flatpak install --user --noninteractive shinpi-ci io.github.yrmuq.shinpi//stable
        # A restored cache may contain an older application than the published baseline.
        flatpak update --user --noninteractive io.github.yrmuq.shinpi
        PREVIOUS_COMMIT=$(ostree --repo=flatpak-repo rev-parse app/io.github.yrmuq.shinpi/x86_64/stable)
        INSTALLED_COMMIT=$(flatpak info --user --show-commit io.github.yrmuq.shinpi)
        test "$INSTALLED_COMMIT" = "$PREVIOUS_COMMIT"
        mkdir -p "$HOME/.var/app/io.github.yrmuq.shinpi/config/shinpi"
        printf '{"language":"ru","panel_width":360}\n' > "$RUNNER_TEMP/previous-settings.json"
        cp "$RUNNER_TEMP/previous-settings.json" "$HOME/.var/app/io.github.yrmuq.shinpi/config/shinpi/config.json"
        ;;
    404) ;;
    *) echo "Cannot read the previous Flatpak signing key: HTTP $STATUS" >&2; exit 1 ;;
esac
