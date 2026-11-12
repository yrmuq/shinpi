"""Reject a zsync file that describes different AppImage bytes."""
import hashlib
from pathlib import Path
import sys


def check(path):
    path = Path(path)
    header = path.with_suffix(path.suffix + ".zsync").read_bytes().split(b"\n\n", 1)[0]
    fields = dict(line.split(": ", 1) for line in header.decode().splitlines() if ": " in line)
    if fields.get("Filename") != path.name or int(fields["Length"]) != path.stat().st_size:
        raise ValueError("zsync filename or size mismatch")
    if fields["SHA-1"] != hashlib.sha1(path.read_bytes()).hexdigest():
        raise ValueError("zsync checksum mismatch")


if __name__ == "__main__":
    check(sys.argv[1])
