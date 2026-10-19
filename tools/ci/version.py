#!/usr/bin/env python3
"""The release version lives in `[workspace.package]` of Cargo.toml; the Flutter app follows it.

  version.py              print the version
  version.py --check      fail unless apps/mobile/pubspec.yaml carries the same version
  version.py --write      copy the version into apps/mobile/pubspec.yaml
  version.py --code       print the Android versionCode (major*1000000 + minor*1000 + patch)
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CARGO = ROOT / "Cargo.toml"
PUBSPEC = ROOT / "apps/mobile/pubspec.yaml"


def workspace_version() -> str:
    section = re.search(r"(?ms)^\[workspace\.package\]\s*$(.*?)(?=^\[|\Z)", CARGO.read_text())
    match = section and re.search(r'(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"\s*$', section.group(1))
    if not match:
        sys.exit("Cargo.toml: no x.y.z version in [workspace.package]")
    return match.group(1)


def pubspec_version() -> str:
    match = re.search(r"(?m)^version:\s*(\d+\.\d+\.\d+)(\+\d+)?\s*$", PUBSPEC.read_text())
    if not match:
        sys.exit("pubspec.yaml: no x.y.z version")
    return match.group(1)


def main() -> None:
    version = workspace_version()
    arg = sys.argv[1] if len(sys.argv) > 1 else ""
    if arg == "--check":
        if pubspec_version() != version:
            sys.exit(f"apps/mobile/pubspec.yaml is {pubspec_version()} but Cargo.toml is {version}; run tools/ci/version.py --write")
    elif arg == "--write":
        text = PUBSPEC.read_text()
        PUBSPEC.write_text(re.sub(r"(?m)^version:\s*\d+\.\d+\.\d+(\+\d+)?\s*$", f"version: {version}+1", text, count=1))
    elif arg == "--code":
        major, minor, patch = map(int, version.split("."))
        print(major * 1_000_000 + minor * 1000 + patch)
    elif arg == "":
        print(version)
    else:
        sys.exit(__doc__)


main()
