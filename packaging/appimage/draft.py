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
    return json.loads(result.stdout)


def main():
    repo = os.environ["GITHUB_REPOSITORY"]
    commit = os.environ["GITHUB_SHA"]
    assets = [Path("dist/shinpi-x86_64.AppImage"), Path("dist/shinpi-x86_64.AppImage.sha256")]
    for asset in assets:
        if not asset.is_file():
            raise RuntimeError(f"Missing build asset: {asset}")
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
    print(release["html_url"])


if __name__ == "__main__":
    main()
