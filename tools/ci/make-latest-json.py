#!/usr/bin/env python3
"""Builds the updater manifest `latest.json` from the signed packages in a directory.

  make-latest-json.py <dir> <version> <download-base-url> <notes> > latest.json

<dir> holds the release assets and their `.sig` files. Both updaters (Tauri's in Mio-Music and
the GPUI app's own) read it: `version`, `notes`, `pub_date` and, per target, `url` and `signature`
(the text of the `.sig` file). Targets use Tauri's keys: `<os>-<arch>` plus `<os>-<arch>-<bundle>`.
"""
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

# asset name suffix -> manifest targets
PACKAGES = {
    ".app.tar.gz": ("darwin-aarch64", "darwin-aarch64-app"),
    "_x64-setup.exe": ("windows-x86_64", "windows-x86_64-nsis"),
    "_amd64.AppImage": ("linux-x86_64", "linux-x86_64-appimage"),
}


def main() -> None:
    if len(sys.argv) != 5:
        sys.exit(__doc__)
    directory, version, base, notes = Path(sys.argv[1]), sys.argv[2], sys.argv[3].rstrip("/"), sys.argv[4]
    platforms = {}
    for suffix, targets in PACKAGES.items():
        packages = sorted(p for p in directory.iterdir() if p.name.endswith(suffix))
        if len(packages) != 1:
            sys.exit(f"expected one *{suffix} in {directory}, found {len(packages)}")
        signature = Path(f"{packages[0]}.sig").read_text().strip()
        entry = {"signature": signature, "url": f"{base}/{packages[0].name}"}
        platforms.update({target: entry for target in targets})
    manifest = {
        "version": version,
        "notes": notes,
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }
    json.dump(manifest, sys.stdout, ensure_ascii=False, indent=2)
    print()


main()
