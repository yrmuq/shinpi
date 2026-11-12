"""Refresh the official appimagetool release URL and checksum in CI."""

import hashlib
import json
from pathlib import Path
import re
import urllib.request


def update(path):
    request = urllib.request.Request(
        "https://api.github.com/repos/AppImage/appimagetool/releases/latest",
        headers={"Accept": "application/vnd.github+json", "User-Agent": "shinpi-pins"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        release = json.load(response)
    text = path.read_text()
    name = "appimagetool-x86_64.AppImage"
    asset = next(a for a in release["assets"] if a["name"] == name)
    url = asset["browser_download_url"]
    if not re.fullmatch(rf"https://github.com/AppImage/appimagetool/releases/download/[^/]+/{re.escape(name)}", url):
        raise ValueError("Unexpected appimagetool download URL")
    digest = hashlib.sha256()
    with urllib.request.urlopen(url, timeout=60) as response:
        for block in iter(lambda: response.read(1024 * 1024), b""):
            digest.update(block)
    sha = digest.hexdigest()
    if asset.get("digest") and asset["digest"] != f"sha256:{sha}":
        raise ValueError("appimagetool checksum does not match GitHub's digest")
    result, urls = re.subn(r'^TOOL_URL="[^"]+"$', f'TOOL_URL="{url}"', text, flags=re.M)
    result, hashes = re.subn(r'^TOOL_SHA="[a-f0-9]+"$', f'TOOL_SHA="{sha}"', result, flags=re.M)
    if (urls, hashes) != (1, 1):
        raise ValueError("Expected exactly one appimagetool pin")
    if result == text:
        return False
    path.write_text(result)
    return True


if __name__ == "__main__":
    update(Path(__file__).resolve().parents[2] / "packaging/appimage/build.sh")
