#!/usr/bin/env python3
"""Check local Markdown links in the project documentation."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
DOCS = [ROOT / "README.md", ROOT / "README.zh_CN.md", *sorted((ROOT / "doc").glob("*.md"))]
FENCE = re.compile(r"^\s{0,3}(`{3,}|~{3,})")
REFERENCE = re.compile(r"^\s{0,3}\[([^\]]+)\]:\s*(?:<([^>]+)>|(\S+))")
LINK = re.compile(r"!?(?:\[[^\]]*\])\((?:<([^>]+)>|([^\s)]+))(?:(?:\s+[^)]*))?\)")
REFERENCE_USE = re.compile(r"!?\[([^\]]*)\](?:\[([^\]]*)\])?")


def visible_lines(text: str) -> list[tuple[int, str]]:
    result = []
    fence_char, fence_size = None, 0
    for number, line in enumerate(text.splitlines(), 1):
        match = FENCE.match(line)
        if match:
            marker = match.group(1)
            if fence_char is None:
                fence_char, fence_size = marker[0], len(marker)
            elif marker[0] == fence_char and len(marker) >= fence_size:
                fence_char = None
            continue
        if fence_char is None:
            result.append((number, line))
    return result


def check(root: Path = ROOT, docs: list[Path] | None = None) -> list[str]:
    errors: list[str] = []
    root = root.resolve()
    for doc in docs if docs is not None else DOCS:
        doc = doc.resolve()
        text = doc.read_text(encoding="utf-8")
        lines = visible_lines(text)
        refs: dict[str, str] = {}
        for _, line in lines:
            found = REFERENCE.match(line)
            if found:
                refs[found.group(1).casefold()] = found.group(2) or found.group(3)
        targets: list[tuple[int, str]] = []
        for number, line in lines:
            if REFERENCE.match(line):
                continue
            spans = []
            for match in LINK.finditer(line):
                target = match.group(1) or match.group(2)
                targets.append((number, target))
                spans.append(match.span())
            # Reference uses that are not inline links; shortcut references too.
            for match in REFERENCE_USE.finditer(line):
                if any(start <= match.start() < end for start, end in spans):
                    continue
                label = match.group(2) or match.group(1)
                target = refs.get(label.casefold())
                if target:
                    targets.append((number, target))
        for number, target in targets:
            parsed = urlsplit(target)
            if parsed.scheme or target.startswith("//") or (not parsed.path and parsed.fragment):
                continue
            path_text = unquote(parsed.path)
            if not path_text:
                continue
            candidate = (doc.parent / path_text).resolve()
            try:
                candidate.relative_to(root)
            except ValueError:
                errors.append(f"{doc.relative_to(root)}:{number}: target escapes repository: {target}")
                continue
            if not candidate.exists():
                errors.append(f"{doc.relative_to(root)}:{number}: missing local target: {target}")
    return errors


if __name__ == "__main__":
    problems = check()
    if problems:
        print("\n".join(problems), file=sys.stderr)
        sys.exit(1)
    print(f"Checked local Markdown links in {len(DOCS)} files.")
