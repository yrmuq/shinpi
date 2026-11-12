"""Attach the CI build to the development draft without creating a Git tag."""

import json
import os
from pathlib import Path
import subprocess


def api(path, method="GET", payload=None):
    command = ["gh", "api", path, "--method", method]
    if payload is not None:
        command.extend(["--input", "-"])
    result = subprocess.run(
        command,
        input=json.dumps(payload) if payload is not None else None,
        text=True,
        capture_output=True,
        check=True,
    )
    return json.loads(result.stdout) if result.stdout.strip() else None


def main():
    repo = os.environ["GITHUB_REPOSITORY"]
    commit = os.environ["GITHUB_SHA"]
    ref = api(f"repos/{repo}/git/ref/heads/dev")
    if ref["object"]["sha"] != commit:
        print("Skipping a superseded development build")
        return
    assets = [Path("dist/shinpi-x86_64.AppImage"), Path("dist/shinpi-x86_64.AppImage.sha256"), Path("dist/shinpi-x86_64.AppImage.zsync")]
    if os.environ.get("INCLUDE_FLATPAK") == "true":
        assets += [Path("dist/shinpi-x86_64.flatpak"), Path("dist/shinpi-x86_64.flatpak.sha256")]
    for asset in assets:
        if not asset.is_file():
            raise RuntimeError(f"Missing build asset: {asset}")
    for asset in assets:
        if asset.suffix == ".sha256":
            subprocess.run(["sha256sum", "-c", asset.name], cwd="dist", check=True)
    releases = []
    page = 1
    while True:
        batch = api(f"repos/{repo}/releases?per_page=100&page={page}")
        releases.extend(batch)
        if len(batch) < 100:
            break
        page += 1
    existing = next((release for release in releases if release["tag_name"] == "dev"), None)
    if existing and not existing["draft"]:
        raise RuntimeError("The dev release is published; refusing to change it")
    payload = {
        "tag_name": "dev",
        "target_commitish": commit,
        "name": "shinpi dev",
        "draft": True,
        "prerelease": True,
        "body": "",
    }
    if existing:
        release = api(f"repos/{repo}/releases/{existing['id']}", "PATCH", payload)
    else:
        release = api(f"repos/{repo}/releases", "POST", payload)
    subprocess.run(
        ["gh", "release", "upload", "dev", *(str(asset) for asset in assets), "--repo", repo, "--clobber"],
        check=True,
    )
    # A later ordinary build must remove optional assets from the previous build.
    expected = {asset.name for asset in assets}
    for asset in api(f"repos/{repo}/releases/{release['id']}/assets"):
        if asset["name"] not in expected:
            api(f"repos/{repo}/releases/assets/{asset['id']}", "DELETE")
    print(release["html_url"])


if __name__ == "__main__":
    main()
