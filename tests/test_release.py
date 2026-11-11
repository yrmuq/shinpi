import os
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

RELEASE = Path(__file__).parents[1] / 'packaging/release'
sys.path.insert(0, str(RELEASE))
import flow
import notes
import pin_tool


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'work'
        self.root.mkdir()
        self.remote = Path(self.temp.name) / 'origin.git'
        self.git('init', '--bare', str(self.remote))
        self.git('init', '-b', 'main')
        self.git('config', 'user.name', 'Test')
        self.git('config', 'user.email', 'test@example.invalid')
        (self.root / 'Cargo.toml').write_text('[package]\nname = "shinpi"\nversion = "0.1.0"\n[dependencies]\nexample = "1"\n')
        (self.root / 'Cargo.lock').write_text('version = 4\n\n[[package]]\nname = "shinpi"\nversion = "0.1.0"\n\n[[package]]\nname = "example"\nversion = "1.0.0"\n')
        action = self.root / '.github/workflows/test.yml'
        action.parent.mkdir(parents=True)
        action.write_text('uses: actions/checkout@v4\n')
        self.commit('Add application')
        self.git('remote', 'add', 'origin', str(self.remote))
        self.git('push', 'origin', 'main')
        self.git('checkout', '-b', 'dev')
        (self.root / 'feature').write_text('feature')
        self.commit('Fix device selection')
        self.git('push', 'origin', 'dev')
        self.git('checkout', 'main')
        self.addCleanup(patch.stopall)
        patch.object(flow, 'ROOT', self.root).start()
        self.output = self.root / 'output'
        patch.dict(os.environ, GITHUB_OUTPUT=str(self.output), GITHUB_REPOSITORY='owner/repo').start()
        self.real_run = flow.run
        self.gh = []
        def run(*args, **kwargs):
            if args[0] == 'gh':
                self.gh.append(args)
                return subprocess.CompletedProcess(args, 0, '', '')
            return self.real_run(*args, **kwargs)
        patch.object(flow, 'run', side_effect=run).start()

    def git(self, *args):
        return subprocess.run(['git', *args], cwd=self.root, text=True, capture_output=True, check=True).stdout.strip()

    def commit(self, subject):
        self.git('add', '.')
        self.git('commit', '-m', subject)

    def outputs(self):
        return dict(line.split('=', 1) for line in self.output.read_text().splitlines())

    def test_stable_release_is_built_from_merge_and_version_returns_to_dev(self):
        self.git('merge', '--no-ff', 'dev', '-m', "Merge branch 'dev'")
        self.git('push', 'origin', 'main')
        flow.prepare('release')
        self.assertEqual(flow.version(), (0, 2, 0))
        self.assertIn('### Fixed\n- Fix device selection.', (self.root / 'notes.md').read_text())
        flow.publish('release', '0.2.0')
        released = self.git('rev-parse', 'HEAD')
        flow.sync_dev()
        self.assertTrue(flow.ancestor(released, 'HEAD'))
        self.assertEqual(flow.version(), (0, 2, 0))
        self.assertEqual(self.git('ls-remote', 'origin', 'refs/heads/dev').split()[0], self.git('rev-parse', 'HEAD'))
        self.assertIn('--latest', self.gh[0])
        self.assertEqual(self.gh[-1][-2:], ('--ref', 'dev'))

    def test_direct_commit_never_bumps_or_tags(self):
        flow.prepare('release')
        self.assertEqual(self.outputs()['ready'], 'false')
        self.assertEqual(flow.version(), (0, 1, 0))
        self.assertEqual(self.git('tag', '--list'), '')

    def test_dependency_updates_accumulate_without_changing_main_or_dev(self):
        stable = self.git('rev-parse', 'main')
        dev = self.git('rev-parse', 'dev')
        manifest = self.root / 'Cargo.toml'
        manifest.write_text(manifest.read_text().replace('example = "1"', 'example = "2"'))
        lock = self.root / 'Cargo.lock'
        lock.write_text(lock.read_text().replace('version = "1.0.0"', 'version = "2.0.0"'))
        self.commit('Update example')
        flow.prepare('deps')
        flow.publish('deps', '0.1.1', self.outputs()['old_deps'])
        self.git('checkout', '--detach', stable)
        action = self.root / '.github/workflows/test.yml'
        action.write_text('uses: actions/checkout@v5\n')
        self.commit('Update actions')
        flow.prepare('deps')
        self.assertEqual(flow.version(), (0, 1, 2))
        self.assertIn('example = "2"', manifest.read_text())
        self.assertIn('@v5', action.read_text())
        flow.publish('deps', '0.1.2', self.outputs()['old_deps'])
        self.assertEqual(self.git('ls-remote', 'origin', 'refs/heads/main').split()[0], stable)
        self.assertEqual(self.git('ls-remote', 'origin', 'refs/heads/dev').split()[0], dev)
        self.assertIn('--prerelease', self.gh[-1])
        # A concurrent writer must not be overwritten by a stale publication.
        published = self.git('rev-parse', 'HEAD')
        (self.root / 'concurrent').write_text('another writer')
        self.commit('Fix another dependency')
        concurrent = self.git('rev-parse', 'HEAD')
        self.git('push', 'origin', 'HEAD:refs/heads/deps')
        self.git('checkout', '--detach', published)
        with self.assertRaises(subprocess.CalledProcessError):
            flow.publish('deps', '0.1.2', published)
        self.assertEqual(self.git('ls-remote', 'origin', 'refs/heads/deps').split()[0], concurrent)

    def test_other_merge_never_publishes_a_stable_release(self):
        self.git('merge', '--no-ff', 'dev', '-m', 'Merge another branch')
        flow.prepare('release')
        self.assertEqual(self.outputs()['ready'], 'false')
        self.assertEqual(flow.version(), (0, 1, 0))

    def test_bump_validates_both_files_before_writing(self):
        manifest = (self.root / 'Cargo.toml').read_text()
        (self.root / 'Cargo.lock').write_text('invalid')
        with self.assertRaises(ValueError):
            flow.bump('0.2.0')
        self.assertEqual((self.root / 'Cargo.toml').read_text(), manifest)


class NoteTests(unittest.TestCase):
    def test_sections_and_dependency_versions(self):
        body = notes.commit_notes(['Add history', 'Fix focus', 'Remove duplicate', 'Version 0.1.1', "Merge branch 'dev'"])
        self.assertIn('### Added\n- Add history.', body)
        self.assertIn('### Fixed\n- Fix focus.', body)
        self.assertIn('### Removed\n- Remove duplicate.', body)
        self.assertNotIn('Version', body)
        self.assertEqual(notes._lock_pairs([' [[package]]', ' name = "example"', '-version = "1.0.0"', '+version = "2.0.0"']), [('example', '1.0.0', '2.0.0')])


class PinTests(unittest.TestCase):
    def test_official_pin_and_checksum_are_updated_together(self):
        payload = b'official tool asset'
        sha = hashlib.sha256(payload).hexdigest()
        url = 'https://github.com/AppImage/appimagetool/releases/download/next/appimagetool-x86_64.AppImage'
        asset = {'name': 'appimagetool-x86_64.AppImage', 'browser_download_url': url, 'digest': 'sha256:' + sha}
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'build.sh'
            initial = 'TOOL_URL="old"\nTOOL_SHA="' + 'a' * 64 + '"\n'
            path.write_text(initial)
            responses = [io.BytesIO(json.dumps({'assets': [asset]}).encode()), io.BytesIO(payload)]
            with patch.object(pin_tool.urllib.request, 'urlopen', side_effect=responses):
                self.assertTrue(pin_tool.update(path))
            self.assertIn('TOOL_URL="' + url + '"', path.read_text())
            self.assertIn('TOOL_SHA="' + sha + '"', path.read_text())
            path.write_text(initial)
            responses = [io.BytesIO(json.dumps({'assets': [asset]}).encode()), io.BytesIO(b'corrupt')]
            with patch.object(pin_tool.urllib.request, 'urlopen', side_effect=responses):
                with self.assertRaisesRegex(ValueError, 'checksum'):
                    pin_tool.update(path)
            self.assertEqual(path.read_text(), initial)


if __name__ == '__main__':
    unittest.main()
