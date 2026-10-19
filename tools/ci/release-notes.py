#!/usr/bin/env python3
"""Release notes for `v<version>`: what changed since the previous release.

  release-notes.py <version>

A hand-written `docs/release-notes/v<version>.md` is used as is when it exists. Otherwise the notes
are the commits since the previous `v*` tag reachable from HEAD (all history before the first
release), grouped by their Conventional Commit type; `- ` lines of a commit body become its
sub-items. Housekeeping types (chore, ci, build, style, test, refactor) are left out.

Both apps show the notes as plain text (the GitHub release body on mobile, `latest.json` on
desktop), so they use plain section titles and `- ` lists that read the same raw and on GitHub.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# Mobile keeps 32 KiB of a release body; stay well inside it.
LIMIT = 24 * 1024
SECTIONS = [
    ("feat", "新功能"),
    ("perf", "优化"),
    ("fix", "修复"),
    ("docs", "文档"),
    (None, "其他更改"),
]
SKIPPED = {"chore", "ci", "build", "style", "test", "refactor"}
SUBJECT = re.compile(r"^(\w+)(?:\([^)]*\))?!?:\s*(.+)$")
TRAILER = re.compile(r"^[A-Za-z-]+: ")


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout


def previous_tag(version: str) -> str | None:
    try:
        tag = git(
            "describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", "--exclude", f"v{version}", "HEAD"
        ).strip()
    except subprocess.CalledProcessError:
        return None
    return tag or None


def items(body: str) -> list[str]:
    """The `- ` lines of a commit body, with their indented continuation lines."""
    lines: list[str] = []
    for line in body.splitlines():
        if TRAILER.match(line):
            continue
        stripped = line.strip()
        if stripped.startswith("- "):
            lines.append(stripped[2:].strip())
        elif lines and line.startswith("  ") and stripped:
            lines[-1] += stripped
    return lines


def generated(version: str) -> str:
    tag = previous_tag(version)
    log = git("log", "--no-merges", "--format=%s%x1f%b%x1e", f"{tag}..HEAD" if tag else "HEAD")
    groups: dict[str | None, list[str]] = {kind: [] for kind, _ in SECTIONS}
    for record in filter(None, (entry.strip("\n") for entry in log.split("\x1e"))):
        subject, _, body = record.partition("\x1f")
        match = SUBJECT.match(subject.strip())
        kind, text = (match.group(1).lower(), match.group(2)) if match else (None, subject.strip())
        if kind in SKIPPED:
            continue
        entry = [f"- {text}", *(f"  - {item}" for item in items(body))]
        groups[kind if kind in groups else None].append("\n".join(entry))
    sections = [
        "\n".join([title, *groups[kind]]) for kind, title in SECTIONS if groups[kind]
    ]
    return "\n\n".join(sections) or "维护性更新与问题修复。"


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    version = sys.argv[1]
    written = ROOT / "docs" / "release-notes" / f"v{version}.md"
    notes = written.read_text(encoding="utf-8").strip() if written.is_file() else generated(version)
    if len(notes.encode("utf-8")) > LIMIT:
        notes = notes.encode("utf-8")[:LIMIT].decode("utf-8", "ignore").rstrip() + "\n…"
    print(notes)


main()
