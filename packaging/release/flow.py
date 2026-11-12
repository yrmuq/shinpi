"""Prepare and publish CI releases; dev builds keep their package version."""

import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

import notes

ROOT = Path(__file__).resolve().parents[2]


def run(*args, check=True):
    return subprocess.run(args, cwd=ROOT, check=check, text=True, capture_output=True)


def git(*args):
    return run("git", *args).stdout.strip()


def output(**values):
    path = os.environ.get("GITHUB_OUTPUT")
    if path:
        with open(path, "a") as stream:
            for key, value in values.items():
                stream.write(f"{key}={value}\n")


def version():
    value = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", value):
        raise ValueError("Package version must be X.Y.Z")
    return tuple(map(int, value.split(".")))


def bump(value):
    if not re.fullmatch(r"\d+\.\d+\.\d+", value):
        raise ValueError("Package version must be X.Y.Z")
    cargo = ROOT / "Cargo.toml"
    lock = ROOT / "Cargo.lock"
    cargo_new, count = re.subn(
        r'(?m)^(version = ")[^"\n]+(")', rf"\g<1>{value}\2", cargo.read_text(), count=1
    )
    lock_new, lock_count = re.subn(
        r'(\[\[package\]\]\nname = "shinpi"\nversion = ")[^"\n]+(")',
        rf"\g<1>{value}\2", lock.read_text(),
    )
    if count != 1 or lock_count != 1:
        raise ValueError("Expected one shinpi package in Cargo.toml and Cargo.lock")
    cargo.write_text(cargo_new)
    lock.write_text(lock_new)


def tag_exists(tag):
    return run("git", "rev-parse", "--verify", f"refs/tags/{tag}", check=False).returncode == 0


def ancestor(first, second):
    return run("git", "merge-base", "--is-ancestor", first, second, check=False).returncode == 0


def fetch():
    git("fetch", "origin", "refs/heads/main:refs/remotes/origin/main",
        "refs/heads/dev:refs/remotes/origin/dev", "--tags")


def prepare(mode):
    fetch()
    git("config", "user.name", "github-actions[bot]")
    git("config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
    old_deps = ""
    if mode == "release":
        parents = git("rev-list", "--parents", "-n", "1", "HEAD").split()
        if (len(parents) != 3
                or git("log", "-1", "--format=%s") != "Merge branch 'dev'"
                or not ancestor(parents[2], "origin/dev")):
            output(ready="false")
            return
        major, minor, _ = version()
        value = f"{major}.{minor + 1}.0"
        stable = git("tag", "--merged", "HEAD", "--list", "v*.*.0", "--sort=-version:refname").splitlines()
        base = stable[0] if stable else ""
        subjects = git("log", "--format=%s", f"{base}..HEAD" if base else "HEAD").splitlines()
        body = notes.commit_notes(subjects)
    else:
        candidate = git("rev-parse", "HEAD")
        remote = run("git", "ls-remote", "origin", "refs/heads/deps").stdout.split()
        if remote:
            old_deps = remote[0]
            git("fetch", "origin", "refs/heads/deps:refs/remotes/origin/deps")
        base = old_deps if old_deps and ancestor("origin/main", old_deps) else "origin/main"
        git("checkout", "--detach", base)
        if mode == "deps":
            git("merge", "--no-ff", candidate, "-m", "Merge dependency updates")
        elif mode == "pins":
            import pin_tool
            if not pin_tool.update(ROOT / "packaging/appimage/build.sh"):
                output(ready="false")
                return
            git("add", "packaging/appimage/build.sh")
            git("commit", "-m", "Update the pinned appimagetool")
        else:
            raise ValueError("Unknown release mode")
        major, minor, patch = version()
        for tag in git("tag", "--list", f"v{major}.{minor}.*").splitlines():
            match = re.fullmatch(rf"v{major}\.{minor}\.(\d+)", tag)
            if match:
                patch = max(patch, int(match[1]))
        value = f"{major}.{minor}.{patch + 1}"
        body = notes.deps_notes(git("diff", "origin/main", "--", "Cargo.toml", "Cargo.lock",
                                    ".github", "packaging/appimage/build.sh"))
    if not body.strip():
        raise ValueError("Release notes are empty")
    tag = f"v{value}"
    if tag_exists(tag):
        raise ValueError(f"Tag {tag} already exists")
    (ROOT / "notes.md").write_text(body)
    bump(value)
    git("add", "Cargo.toml", "Cargo.lock")
    git("commit", "-m", f"Version {value}")
    git("tag", "-a", tag, "-F", "notes.md")
    output(ready="true", version=value, old_deps=old_deps)


def publish(mode, value, old_deps=""):
    if not re.fullmatch(r"\d+\.\d+\.\d+", value):
        raise ValueError("Invalid version")
    if version() != tuple(map(int, value.split("."))):
        raise ValueError("Build version does not match the release")
    tag = f"v{value}"
    if git("rev-parse", f"{tag}^{{commit}}") != git("rev-parse", "HEAD"):
        raise ValueError("Tag does not match the build commit")
    if mode == "release":
        git("push", "--atomic", "origin", "HEAD:refs/heads/main", f"refs/tags/{tag}")
    elif mode in ("deps", "pins"):
        # Only deps may be replaced when a new stable release supersedes its base.
        git("push", "--atomic", f"--force-with-lease=refs/heads/deps:{old_deps}", "origin",
            "HEAD:refs/heads/deps", f"refs/tags/{tag}")
    else:
        raise ValueError("Unknown release mode")
    command = ["gh", "release", "create", tag,
               *(str(path) for path in sorted((ROOT / "dist").glob("*")) if path.is_file()), "--repo", os.environ["GITHUB_REPOSITORY"],
               "--verify-tag", "--title", f"shinpi {value}", "--notes-file", "notes.md"]
    command += ["--latest"] if mode == "release" else ["--prerelease", "--latest=false"]
    run(*command)


def sync_dev():
    released = git("rev-parse", "HEAD")
    git("fetch", "origin", "refs/heads/dev:refs/remotes/origin/dev")
    if ancestor(released, "origin/dev"):
        return
    git("checkout", "--detach", "origin/dev")
    git("merge", "--no-ff", released, "-m", "Merge released main into dev")
    git("push", "origin", "HEAD:refs/heads/dev")
    # GITHUB_TOKEN pushes do not trigger push workflows.
    run("gh", "workflow", "run", "dev.yml", "--repo", os.environ["GITHUB_REPOSITORY"], "--ref", "dev")


if __name__ == "__main__":
    os.chdir(ROOT)
    try:
        if sys.argv[1] == "prepare":
            prepare(sys.argv[2])
        elif sys.argv[1] == "publish":
            publish(*sys.argv[2:])
        elif sys.argv[1] == "sync-dev":
            sync_dev()
        else:
            raise SystemExit("Expected prepare, publish or sync-dev")
    except subprocess.CalledProcessError as error:
        print(error.stderr, file=sys.stderr)
        raise SystemExit(error.returncode)
