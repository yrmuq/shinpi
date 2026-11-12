// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::{json, Value};
use shinpi_tools::{flow, Context};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static NEXT: AtomicU64 = AtomicU64::new(0);
static STUBS: OnceLock<PathBuf> = OnceLock::new();

fn stubs() -> &'static PathBuf {
    STUBS.get_or_init(|| {
        let path = std::env::temp_dir().join(format!("shinpi-stubs-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        for (name, source) in [("gh", GH), ("curl", CURL)] {
            fs::write(path.join(name), source).unwrap();
            fs::set_permissions(path.join(name), fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    })
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        // Publish executable stubs once before any parallel test can fork.
        stubs();
        let root = std::env::temp_dir().join(format!(
            "shinpi-tools-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let fixture = Self { root };
        fixture.write(
            "ref.json",
            &json!({"object":{"sha":"a".repeat(40)}}).to_string(),
        );
        fixture.write("releases.json", "[]");
        fixture.write("assets.json", "[]");
        fixture.write(
            "release.json",
            r#"{"id":7,"html_url":"https://github.com/owner/repo/releases/7"}"#,
        );
        fixture
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.root.join(path)).unwrap()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_shinpi-tools"));
        command
            .current_dir(&self.root)
            .args(args)
            .env(
                "PATH",
                format!("{}:{}", stubs().display(), std::env::var("PATH").unwrap()),
            )
            .env("STUB_DIR", &self.root)
            .env("GITHUB_REPOSITORY", "owner/repo")
            .env("GITHUB_SHA", "a".repeat(40))
            .env("GITHUB_OUTPUT", self.root.join("output"))
            .env("INCLUDE_FLATPAK", "false");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        let output = self.command(args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn fails(&self, args: &[&str], message: &str) {
        let output = self.command(args).output().unwrap();
        assert!(!output.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn commit(&self, subject: &str) {
        self.git(&["add", "Cargo.toml", "Cargo.lock", ".github"]);
        self.git(&["commit", "-m", subject]);
    }

    fn repository() -> Self {
        let fixture = Self::new();
        fixture.git(&["init", "--bare", "origin.git"]);
        fixture.git(&["init", "-b", "main"]);
        fixture.git(&["config", "user.name", "Test"]);
        fixture.git(&["config", "user.email", "test@example.invalid"]);
        fixture.write(
            "Cargo.toml",
            "[package]\nname = \"shinpi\"\nversion = \"0.1.0\"\n[dependencies]\nexample = \"1\"\n",
        );
        fixture.write("Cargo.lock", "version = 4\n\n[[package]]\nname = \"shinpi\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"example\"\nversion = \"1.0.0\"\n");
        fixture.write(".github/workflows/test.yml", "uses: actions/checkout@v4\n");
        fixture.commit("Add application");
        fixture.git(&["remote", "add", "origin", "origin.git"]);
        fixture.git(&["push", "origin", "main"]);
        fixture.git(&["checkout", "-b", "dev"]);
        fixture.write(".github/workflows/feature.yml", "feature\n");
        fixture.commit("Fix device selection");
        fixture.git(&["push", "origin", "dev"]);
        fixture.git(&["checkout", "main"]);
        fixture.write("dist/package", "test package");
        fixture
    }

    fn assets(&self) {
        self.write("dist/shinpi-x86_64.AppImage", "image");
        self.write("dist/shinpi-x86_64.AppImage.zsync", "zsync");
        let output = Command::new("sha256sum")
            .current_dir(self.root.join("dist"))
            .args(["shinpi-x86_64.AppImage", "shinpi-x86_64.AppImage.zsync"])
            .output()
            .unwrap();
        self.write(
            "dist/shinpi-x86_64.AppImage.sha256",
            std::str::from_utf8(&output.stdout).unwrap(),
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const GH: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$STUB_DIR/gh.log"
case "$1" in
    api)
        case "$*" in
            *'--input -'*) cat > "$STUB_DIR/payload.json"; cat "$STUB_DIR/release.json" ;;
            *'--method DELETE'*) ;;
            *'/git/ref/heads/dev'*) cat "$STUB_DIR/ref.json" ;;
            *'/assets '*) cat "$STUB_DIR/assets.json" ;;
            *'/releases?'*) cat "$STUB_DIR/releases.json" ;;
            *) exit 2 ;;
        esac
        ;;
    release|workflow) ;;
    *) exit 2 ;;
esac
"#;

const CURL: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$STUB_DIR/curl.log"
case "$*" in
    *'/releases/latest'*) cat "$STUB_DIR/tool-release.json" ;;
    *)
        while [ "$#" -gt 0 ]; do
            if [ "$1" = --output ]; then cp "$STUB_DIR/tool-asset" "$2"; exit; fi
            shift
        done
        exit 2
        ;;
esac
"#;

#[test]
fn stable_release_requires_merge_and_syncs_version_back_to_dev() {
    let fixture = Fixture::repository();
    fixture.git(&["merge", "--no-ff", "dev", "-m", "Merge branch 'dev'"]);
    fixture.git(&["push", "origin", "main"]);
    fixture.run(&["prepare", "release"]);
    assert!(fixture.read("output").contains("version=0.2.0"));
    assert!(fixture
        .read("notes.md")
        .contains("### Fixed\n- Fix device selection."));
    fixture.run(&["publish", "release", "0.2.0"]);
    let released = fixture.git(&["rev-parse", "HEAD"]);
    fixture.run(&["sync-dev"]);
    fixture.git(&["merge-base", "--is-ancestor", &released, "HEAD"]);
    assert!(fixture.read("Cargo.toml").contains("version = \"0.2.0\""));
    assert_eq!(
        fixture
            .git(&["ls-remote", "origin", "refs/heads/dev"])
            .split_whitespace()
            .next()
            .unwrap(),
        fixture.git(&["rev-parse", "HEAD"])
    );
    assert!(fixture.read("gh.log").contains("--latest\n"));
    assert!(fixture
        .read("gh.log")
        .contains("workflow run dev.yml --repo owner/repo --ref dev"));
}

#[test]
fn direct_main_commit_never_bumps_tags_or_publishes() {
    let fixture = Fixture::repository();
    fixture.write(".github/workflows/test.yml", "uses: actions/checkout@v5\n");
    fixture.commit("Move build tools to Rust");
    fixture.run(&["prepare", "release"]);
    assert_eq!(fixture.read("output"), "ready=false\n");
    assert!(fixture.read("Cargo.toml").contains("version = \"0.1.0\""));
    assert_eq!(fixture.git(&["tag", "--list"]), "");
    assert!(!fixture.root.join("gh.log").exists());
}

#[test]
fn other_merge_does_not_publish() {
    let fixture = Fixture::repository();
    fixture.git(&["merge", "--no-ff", "dev", "-m", "Merge another branch"]);
    fixture.run(&["prepare", "release"]);
    assert_eq!(fixture.read("output"), "ready=false\n");
    assert_eq!(fixture.git(&["tag", "--list"]), "");
}

#[test]
fn dependencies_accumulate_preserve_main_dev_and_reject_concurrent_publication() {
    let fixture = Fixture::repository();
    let stable = fixture.git(&["rev-parse", "main"]);
    let dev = fixture.git(&["rev-parse", "dev"]);
    fixture.write(
        "Cargo.toml",
        &fixture
            .read("Cargo.toml")
            .replace("example = \"1\"", "example = \"2\""),
    );
    fixture.write(
        "Cargo.lock",
        &fixture
            .read("Cargo.lock")
            .replace("version = \"1.0.0\"", "version = \"2.0.0\""),
    );
    fixture.commit("Update example");
    fixture.run(&["prepare", "deps"]);
    fixture.run(&["publish", "deps", "0.1.1", ""]);
    fixture.git(&["checkout", "--detach", &stable]);
    fixture.write(".github/workflows/test.yml", "uses: actions/checkout@v5\n");
    fixture.commit("Update actions");
    fixture.run(&["prepare", "deps"]);
    assert!(fixture.read("output").contains("version=0.1.2"));
    assert!(fixture.read("Cargo.toml").contains("example = \"2\""));
    assert!(fixture.read(".github/workflows/test.yml").contains("@v5"));
    let outputs = fixture.read("output");
    let old_deps = outputs
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("old_deps="))
        .unwrap();
    fixture.run(&["publish", "deps", "0.1.2", old_deps]);
    assert_eq!(
        fixture
            .git(&["ls-remote", "origin", "refs/heads/main"])
            .split_whitespace()
            .next()
            .unwrap(),
        stable
    );
    assert_eq!(
        fixture
            .git(&["ls-remote", "origin", "refs/heads/dev"])
            .split_whitespace()
            .next()
            .unwrap(),
        dev
    );
    assert!(fixture
        .read("gh.log")
        .contains("--prerelease --latest=false"));
    let published = fixture.git(&["rev-parse", "HEAD"]);
    fixture.write(".github/concurrent", "another writer");
    fixture.commit("Fix another dependency");
    let concurrent = fixture.git(&["rev-parse", "HEAD"]);
    fixture.git(&["push", "origin", "HEAD:refs/heads/deps"]);
    fixture.git(&["checkout", "--detach", &published]);
    fixture.fails(&["publish", "deps", "0.1.2", &published], "stale info");
    assert_eq!(
        fixture
            .git(&["ls-remote", "origin", "refs/heads/deps"])
            .split_whitespace()
            .next()
            .unwrap(),
        concurrent
    );
}

#[test]
fn bump_validates_both_files_before_writing() {
    let fixture = Fixture::repository();
    let manifest = fixture.read("Cargo.toml");
    fixture.write("Cargo.lock", "invalid");
    assert!(flow::bump(
        &Context {
            root: fixture.root.clone()
        },
        "0.2.0"
    )
    .is_err());
    assert_eq!(fixture.read("Cargo.toml"), manifest);
}

#[test]
fn publication_rejects_wrong_version_and_wrong_tag_commit() {
    let fixture = Fixture::repository();
    fixture.fails(
        &["publish", "release", "0.2.0"],
        "Build version does not match",
    );
    fixture.git(&["tag", "v0.1.0", "dev"]);
    fixture.fails(&["publish", "release", "0.1.0"], "Tag does not match");
    assert!(!fixture.root.join("gh.log").exists());
}

#[test]
fn published_dev_release_is_never_modified() {
    let fixture = Fixture::new();
    fixture.assets();
    fixture.write(
        "releases.json",
        r#"[{"tag_name":"dev","draft":false,"id":7}]"#,
    );
    fixture.fails(&["draft"], "refusing to change");
    assert!(!fixture.root.join("payload.json").exists());
    assert!(!fixture.read("gh.log").contains("release upload"));
}

#[test]
fn draft_is_created_only_for_built_commit_without_a_tag() {
    let fixture = Fixture::new();
    fixture.assets();
    fixture.run(&["draft"]);
    let payload: Value = serde_json::from_str(&fixture.read("payload.json")).unwrap();
    assert_eq!(payload["draft"], true);
    assert_eq!(payload["prerelease"], true);
    assert_eq!(payload["target_commitish"], "a".repeat(40));
    assert_eq!(payload["body"], "");
    assert!(fixture.read("gh.log").contains("release upload dev"));
    assert!(!fixture.read("gh.log").contains("release create"));
}

#[test]
fn superseded_build_never_touches_draft() {
    let fixture = Fixture::new();
    fixture.write(
        "ref.json",
        &json!({"object":{"sha":"b".repeat(40)}}).to_string(),
    );
    fixture.run(&["draft"]);
    assert_eq!(fixture.read("gh.log").lines().count(), 1);
    assert!(!fixture.root.join("payload.json").exists());
}

#[test]
fn ordinary_build_removes_previous_optional_bundle() {
    let fixture = Fixture::new();
    fixture.assets();
    fixture.write(
        "releases.json",
        r#"[{"tag_name":"dev","draft":true,"id":7}]"#,
    );
    fixture.write(
        "assets.json",
        r#"[{"name":"shinpi-x86_64.flatpak","id":8},{"name":"shinpi-x86_64.AppImage","id":9}]"#,
    );
    fixture.run(&["draft"]);
    let log = fixture.read("gh.log");
    assert!(log.contains("releases/assets/8 --method DELETE"));
    assert!(!log.contains("releases/assets/9 --method DELETE"));
    assert!(!log
        .lines()
        .find(|line| line.starts_with("release upload"))
        .unwrap()
        .contains("flatpak"));
}

#[test]
fn missing_assets_or_bad_checksums_block_draft() {
    let fixture = Fixture::new();
    fixture.fails(&["draft"], "Missing build asset");
    fixture.assets();
    fixture.write("dist/shinpi-x86_64.AppImage", "corrupt");
    fixture.fails(&["draft"], "Command failed");
    assert!(!fixture.root.join("payload.json").exists());
}

#[test]
fn flatpak_bundle_requires_explicit_selection_and_matching_checksums() {
    let fixture = Fixture::new();
    fixture.assets();
    let output = fixture
        .command(&["draft"])
        .env("INCLUDE_FLATPAK", "true")
        .output()
        .unwrap();
    assert!(!output.status.success());
    fixture.write("dist/shinpi-x86_64.flatpak", "flatpak");
    let output = Command::new("sha256sum")
        .current_dir(fixture.root.join("dist"))
        .arg("shinpi-x86_64.flatpak")
        .output()
        .unwrap();
    fixture.write(
        "dist/shinpi-x86_64.flatpak.sha256",
        std::str::from_utf8(&output.stdout).unwrap(),
    );
    let output = fixture
        .command(&["draft"])
        .env("INCLUDE_FLATPAK", "true")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture
        .read("gh.log")
        .contains("dist/shinpi-x86_64.flatpak dist/shinpi-x86_64.flatpak.sha256"));
}

#[test]
fn zsync_detects_replaced_image_even_with_same_size() {
    let fixture = Fixture::new();
    fixture.write("image.AppImage", "original");
    let output = Command::new("sha1sum")
        .arg(fixture.root.join("image.AppImage"))
        .output()
        .unwrap();
    let digest = std::str::from_utf8(&output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    fixture.write(
        "image.AppImage.zsync",
        &format!("Filename: image.AppImage\nLength: 8\nSHA-1: {digest}\n\n"),
    );
    fixture.run(&["check-zsync", "image.AppImage"]);
    fixture.write("image.AppImage", "replaced");
    fixture.fails(&["check-zsync", "image.AppImage"], "checksum mismatch");
    fixture.write("image.AppImage", "short");
    fixture.fails(&["check-zsync", "image.AppImage"], "size mismatch");
    fixture.write(
        "image.AppImage.zsync",
        &format!("Filename: other.AppImage\nLength: 5\nSHA-1: {digest}\n\n"),
    );
    fixture.fails(
        &["check-zsync", "image.AppImage"],
        "filename or size mismatch",
    );
}

fn pin_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.write("tool-asset", "official tool asset");
    let output = Command::new("sha256sum")
        .arg(fixture.root.join("tool-asset"))
        .output()
        .unwrap();
    let digest = std::str::from_utf8(&output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    let url = "https://github.com/AppImage/appimagetool/releases/download/next/appimagetool-x86_64.AppImage";
    fixture.write("tool-release.json", &json!({"assets":[{"name":"appimagetool-x86_64.AppImage","browser_download_url":url,"digest":format!("sha256:{digest}")}]}).to_string());
    fixture.write(
        "packaging/appimage/build.sh",
        &format!("TOOL_URL=\"old\"\nTOOL_SHA=\"{}\"\n", "a".repeat(64)),
    );
    fixture
}

#[test]
fn tool_pin_and_checksum_update_together() {
    let fixture = pin_fixture();
    fixture.run(&["pin-tool"]);
    let text = fixture.read("packaging/appimage/build.sh");
    assert!(text
        .contains("TOOL_URL=\"https://github.com/AppImage/appimagetool/releases/download/next/"));
    assert!(!text.contains(&"a".repeat(64)));
    assert!(!fixture
        .root
        .join("packaging/appimage/appimagetool.pin-download")
        .exists());
    fixture.run(&["pin-tool"]);
    assert_eq!(fixture.read("packaging/appimage/build.sh"), text);
}

#[test]
fn corrupt_download_or_unexpected_url_never_changes_pin() {
    let fixture = pin_fixture();
    let initial = fixture.read("packaging/appimage/build.sh");
    fixture.write("tool-asset", "corrupt");
    fixture.fails(&["pin-tool"], "checksum does not match");
    assert_eq!(fixture.read("packaging/appimage/build.sh"), initial);
    fixture.write(
        "tool-release.json",
        &fixture
            .read("tool-release.json")
            .replace("https://github.com/", "https://example.invalid/"),
    );
    fixture.fails(&["pin-tool"], "Unexpected appimagetool download URL");
    assert_eq!(fixture.read("packaging/appimage/build.sh"), initial);
}

#[test]
fn flatpakref_preserves_update_identity_and_public_key() {
    let fixture = Fixture::new();
    fixture.write("flatpak-repo/shinpi.gpg", "public key");
    fs::create_dir(fixture.root.join("dist")).unwrap();
    fixture.run(&["flatpakref"]);
    let text = fixture.read("dist/shinpi.flatpakref");
    for line in [
        "Name=io.github.yrmuq.shinpi\n",
        "Branch=stable\n",
        "Url=https://yrmuq.github.io/shinpi/\n",
        "GPGKey=cHVibGljIGtleQ==\n",
        "RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo\n",
    ] {
        assert!(text.contains(line), "{text}");
    }
}

#[test]
fn invalid_readings_fail_the_cli() {
    let fixture = Fixture::new();
    for (input, success) in [
        (r#"{"devices":[],"warnings":[]}"#, true),
        (r#"{"devices":[],"warnings":{}}"#, false),
    ] {
        let mut child = fixture
            .command(&["check-readings"])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        assert_eq!(child.wait_with_output().unwrap().status.success(), success);
    }
}
