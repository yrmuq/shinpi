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
# Keep signed history so installed applications can update without replacing their remote.
STATUS=$(curl -L --retry 3 -o "$RUNNER_TEMP/previous.gpg" -w '%{http_code}' https://yrmuq.github.io/shinpi/shinpi.gpg)
case "$STATUS" in
    200)
        cmp "$RUNNER_TEMP/shinpi.gpg" "$RUNNER_TEMP/previous.gpg"
        ostree --repo=flatpak-repo init --mode=archive
        ostree --repo=flatpak-repo remote add --gpg-import="$RUNNER_TEMP/shinpi.gpg" previous https://yrmuq.github.io/shinpi/
        ostree --repo=flatpak-repo pull --mirror previous
        flatpak remote-add --user --if-not-exists --gpg-import="$RUNNER_TEMP/shinpi.gpg" shinpi-ci "file://$PWD/flatpak-repo"
        flatpak install --user --noninteractive shinpi-ci io.github.yrmuq.shinpi//stable
        mkdir -p "$HOME/.var/app/io.github.yrmuq.shinpi/config"
        printf 'settings preservation check\n' > "$HOME/.var/app/io.github.yrmuq.shinpi/config/ci-marker"
        ;;
    404) ;;
    *) echo "Cannot read the previous Flatpak signing key: HTTP $STATUS" >&2; exit 1 ;;
esac
