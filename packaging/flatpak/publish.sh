#!/bin/sh
set -eu
test -n "${FLATPAK_GPG_ID:?}"
flatpak build-update-repo --gpg-sign="$FLATPAK_GPG_ID" flatpak-repo
gpg --export "$FLATPAK_GPG_ID" > flatpak-repo/shinpi.gpg
python3 - <<'PY'
import base64
from pathlib import Path
key = base64.b64encode(Path('flatpak-repo/shinpi.gpg').read_bytes()).decode()
Path('dist/shinpi.flatpakref').write_text(f'''[Flatpak Ref]
Name=io.github.yrmuq.shinpi
Branch=stable
Title=shinpi
Url=https://yrmuq.github.io/shinpi/
SuggestRemoteName=shinpi
GPGKey={key}
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
IsRuntime=false
''')
PY
(cd dist && sha256sum shinpi.flatpakref > shinpi.flatpakref.sha256)
