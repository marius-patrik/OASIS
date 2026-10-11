#!/usr/bin/env python3
"""Fail closed on unreviewed tracked game/media assets.

Checks only OASIS top-level git-tracked files, not nested upstream submodules
or produced release archives. Submodule/release rights require separate audit.
"""
import hashlib
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "tests" / "approved_assets.json"
GAME_BINARY_SUFFIXES = {
    ".wad", ".pk3", ".pk4", ".pak", ".pck", ".sf2",
    ".ogg", ".mp3", ".wav", ".flac", ".png", ".jpg", ".jpeg",
    ".bmp", ".gif", ".webp",
}
DISALLOWED_NAMES = {"doom1.wad", "doom.wad", "doom2.wad", "cavestory+.exe"}
DENY_LICENSES = {"", "UNKNOWN", "NOASSERTION", "PROPRIETARY", "UNLICENSED"}


def main() -> int:
    entries = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=ROOT)
    actual = set()
    for entry in entries.split(b"\0"):
        if not entry:
            continue
        metadata, path_bytes = entry.split(b"\t", 1)
        mode = metadata.split(b" ", 1)[0]
        path = path_bytes.decode("utf-8")
        if mode == b"160000":
            # Gitlink is an upstream repository, not a licensed OASIS asset.
            continue
        asset = pathlib.PurePosixPath(path)
        if asset.suffix.lower() in GAME_BINARY_SUFFIXES or asset.name.lower() in DISALLOWED_NAMES:
            actual.add(path)

    declared = json.loads(MANIFEST.read_text(encoding="utf-8"))["assets"]
    approved = {}
    problems = []
    for item in declared:
        path = item.get("path", "")
        lic = item.get("license", "").upper()
        h = item.get("sha256", "")
        source = item.get("source", "")
        if path in approved:
            problems.append(f"duplicate manifest entry: {path}")
        approved[path] = item
        if not path.startswith("tests/fixtures/licensed/"):
            problems.append(f"asset must live under tests/fixtures/licensed/: {path}")
        if lic in DENY_LICENSES or not source or len(h) != 64:
            problems.append(f"asset lacks independently reviewed provenance/license/hash: {path}")
        if pathlib.PurePosixPath(path).name.lower() in DISALLOWED_NAMES:
            problems.append(f"proprietary game asset not approved for bundle: {path}")
        candidate = ROOT / path
        if not candidate.is_file():
            problems.append(f"manifest references missing asset: {path}")
        elif hashlib.sha256(candidate.read_bytes()).hexdigest() != h:
            problems.append(f"asset hash mismatch: {path}")
    for path in sorted(actual - set(approved)):
        problems.append(f"unreviewed game/media asset tracked by OASIS: {path}")
    for path in sorted(set(approved) - actual):
        problems.append(f"manifest entry is not a tracked media asset: {path}")
    if problems:
        for error in problems:
            print(error, file=sys.stderr)
        return 1
    print(f"Top-level tracked game/media allowlist clear ({len(actual)} independently declared assets).")
    print("Does not certify upstream submodules, derived packages, or combined GPL distribution.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
