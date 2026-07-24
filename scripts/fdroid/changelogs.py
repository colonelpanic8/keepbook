#!/usr/bin/env python3
"""Generate fastlane changelog files from CHANGELOG.md.

F-Droid, IzzyOnDroid, and the self-hosted repo all read per-release notes from
``fastlane/metadata/android/<locale>/changelogs/<versionCode>.txt``. Those files
are derived here rather than hand-written so that a release only ever has to
touch CHANGELOG.md.

The version code convention matches ``androidVersionCode`` in flake.nix:
``major * 10000 + minor * 100 + patch``.

Usage:
    scripts/fdroid/changelogs.py [--check]

``--check`` writes nothing and exits non-zero if the committed changelogs are
stale, which is what CI runs.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
CHANGELOG = REPO_ROOT / "CHANGELOG.md"
OUT_DIR = REPO_ROOT / "fastlane/metadata/android/en-US/changelogs"

# F-Droid truncates long release notes in some clients; keep entries short
# enough to display in full.
MAX_CHARS = 500

RELEASE_HEADING = re.compile(r"^##\s+(\d+)\.(\d+)\.(\d+)\b")
SECTION_HEADING = re.compile(r"^###\s+(.+?)\s*$")


def version_code(major: int, minor: int, patch: int) -> int:
    return major * 10000 + minor * 100 + patch


def parse_releases(text: str) -> dict[int, str]:
    """Map version code -> rendered changelog body."""
    releases: dict[int, str] = {}
    code: int | None = None
    section: str | None = None
    entries: list[str] = []

    def flush() -> None:
        if code is not None:
            releases[code] = render(entries)

    for line in text.splitlines():
        heading = RELEASE_HEADING.match(line)
        if heading:
            flush()
            code = version_code(*(int(part) for part in heading.groups()))
            section = None
            entries = []
            continue

        if code is None:
            continue

        section_match = SECTION_HEADING.match(line)
        if section_match:
            section = section_match.group(1)
            continue

        if line.startswith("- "):
            entries.append(f"{section}: {line[2:].strip()}" if section else line[2:].strip())
        elif entries and line.startswith("  ") and line.strip():
            # Continuation of a wrapped bullet.
            entries[-1] = f"{entries[-1]} {line.strip()}"

    flush()
    return releases


def render(entries: list[str]) -> str:
    if not entries:
        return "Maintenance release.\n"

    lines: list[str] = []
    used = 0
    for entry in entries:
        candidate = f"* {entry}"
        # +1 for the newline each line contributes.
        if lines and used + len(candidate) + 1 > MAX_CHARS:
            lines.append("* ...see CHANGELOG.md for the rest.")
            break
        lines.append(candidate)
        used += len(candidate) + 1

    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify committed changelogs are up to date instead of writing them",
    )
    args = parser.parse_args()

    releases = parse_releases(CHANGELOG.read_text(encoding="utf-8"))
    if not releases:
        print(f"no releases parsed from {CHANGELOG}", file=sys.stderr)
        return 1

    stale: list[str] = []
    for code, body in sorted(releases.items()):
        target = OUT_DIR / f"{code}.txt"
        current = target.read_text(encoding="utf-8") if target.exists() else None
        if current == body:
            continue
        if args.check:
            stale.append(target.relative_to(REPO_ROOT).as_posix())
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(body, encoding="utf-8")

    # Changelogs for versions no longer in CHANGELOG.md would otherwise linger.
    expected = {f"{code}.txt" for code in releases}
    if OUT_DIR.exists():
        for orphan in sorted(OUT_DIR.glob("*.txt")):
            if orphan.name in expected:
                continue
            if args.check:
                stale.append(orphan.relative_to(REPO_ROOT).as_posix())
            else:
                orphan.unlink()

    if stale:
        print("fastlane changelogs are out of date:", file=sys.stderr)
        for path in stale:
            print(f"  {path}", file=sys.stderr)
        print("run: scripts/fdroid/changelogs.py", file=sys.stderr)
        return 1

    if not args.check:
        print(f"wrote {len(releases)} changelog(s) to {OUT_DIR.relative_to(REPO_ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
