import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "draft", Path(__file__).parents[1] / "packaging/appimage/draft.py"
)
draft = importlib.util.module_from_spec(spec)
spec.loader.exec_module(draft)


class DraftTests(unittest.TestCase):
    @patch.dict("os.environ", GITHUB_REPOSITORY="owner/repo", GITHUB_SHA="a" * 40)
    @patch.object(Path, "is_file", return_value=True)
    @patch.object(draft.subprocess, "run")
    @patch.object(draft, "api")
    def test_published_dev_release_is_never_modified(self, api, upload, _):
        api.side_effect = [{"object": {"sha": "a" * 40}}, [{"tag_name": "dev", "draft": False, "id": 7}]]
        with self.assertRaisesRegex(RuntimeError, "refusing to change"):
            draft.main()
        self.assertEqual(api.call_count, 2)
        self.assertEqual(upload.call_args.args[0][0], "sha256sum")

    @patch.dict("os.environ", GITHUB_REPOSITORY="owner/repo", GITHUB_SHA="a" * 40)
    @patch.object(Path, "is_file", return_value=True)
    @patch.object(draft.subprocess, "run")
    @patch.object(draft, "api")
    def test_creates_only_a_draft_for_the_built_commit(self, api, upload, _):
        api.side_effect = [{"object": {"sha": "a" * 40}}, [], {"id": 1, "html_url": "https://github.com/owner/repo/releases/1"}, []]
        draft.main()
        path, method, payload = api.call_args_list[2].args
        self.assertEqual((path, method), ("repos/owner/repo/releases", "POST"))
        self.assertTrue(payload["draft"])
        self.assertEqual(payload["target_commitish"], "a" * 40)
        self.assertEqual(payload["body"], "")
        self.assertEqual(upload.call_count, 2)
        self.assertEqual(upload.call_args.args[0][:3], ["gh", "release", "upload"])

    @patch.dict("os.environ", GITHUB_REPOSITORY="owner/repo", GITHUB_SHA="a" * 40)
    @patch.object(draft.subprocess, "run")
    @patch.object(draft, "api")
    def test_superseded_build_never_touches_the_draft(self, api, upload):
        api.return_value = {"object": {"sha": "b" * 40}}
        draft.main()
        api.assert_called_once()
        upload.assert_not_called()

    @patch.dict("os.environ", GITHUB_REPOSITORY="owner/repo", GITHUB_SHA="a" * 40, INCLUDE_FLATPAK="false")
    @patch.object(Path, "is_file", return_value=True)
    @patch.object(draft.subprocess, "run")
    @patch.object(draft, "api")
    def test_ordinary_build_removes_previous_optional_bundle(self, api, upload, _):
        api.side_effect = [
            {"object": {"sha": "a" * 40}},
            [{"tag_name": "dev", "draft": True, "id": 7}],
            {"id": 7, "html_url": "draft"},
            [{"name": "shinpi-x86_64.flatpak", "id": 8},
             {"name": "shinpi-x86_64.AppImage", "id": 9}],
            None,
        ]
        draft.main()
        self.assertEqual(api.call_args.args, ("repos/owner/repo/releases/assets/8", "DELETE"))
        self.assertNotIn("flatpak", " ".join(upload.call_args.args[0]))
