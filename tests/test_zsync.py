import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('zsync_check', Path(__file__).parents[1] / 'packaging/appimage/check_zsync.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ZsyncTests(unittest.TestCase):
    def test_detects_replaced_appimage_even_with_same_size(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'shinpi-x86_64.AppImage'
            path.write_bytes(b'original')
            digest = hashlib.sha1(path.read_bytes()).hexdigest()
            path.with_suffix('.AppImage.zsync').write_bytes(f'Filename: {path.name}\nLength: 8\nSHA-1: {digest}\n\n'.encode())
            module.check(path)
            path.write_bytes(b'replaced')
            with self.assertRaisesRegex(ValueError, 'checksum'):
                module.check(path)
