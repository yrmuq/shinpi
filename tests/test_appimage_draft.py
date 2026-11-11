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
        api.return_value = [{"tag_name": "dev", "draft": False, "id": 7}]
        with self.assertRaisesRegex(RuntimeError, "refusing to change"):
            draft.main()
        self.assertEqual(api.call_count, 1)
        upload.assert_not_called()

    @patch.dict("os.environ", GITHUB_REPOSITORY="owner/repo", GITHUB_SHA="a" * 40)
    @patch.object(Path, "is_file", return_value=True)
    @patch.object(draft.subprocess, "run")
    @patch.object(draft, "api")
    def test_creates_only_a_draft_for_the_built_commit(self, api, upload, _):
        api.side_effect = [[], {"html_url": "https://github.com/owner/repo/releases/1"}]
        draft.main()
        path, method, payload = api.call_args.args
        self.assertEqual((path, method), ("repos/owner/repo/releases", "POST"))
        self.assertTrue(payload["draft"])
        self.assertEqual(payload["target_commitish"], "a" * 40)
        self.assertEqual(payload["body"], "")
        upload.assert_called_once()
