#!/usr/bin/env python3
from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


@dataclass(frozen=True)
class Todo:
    path: Path
    lineno: int
    text: str


def iter_plan_files(plan_dir: Path) -> Iterable[Path]:
    if not plan_dir.exists():
        return []
    return sorted([p for p in plan_dir.glob("*.md") if p.is_file()])


def first_unchecked_todo(path: Path, contains: str | None) -> Todo | None:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except Exception:
        return None

    for i, line in enumerate(lines, start=1):
        if "- [ ]" not in line:
            continue
        if contains and contains.lower() not in line.lower():
            continue
        return Todo(path=path, lineno=i, text=line.strip())
    return None


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Print the next unchecked TODO from docs/plan (read-only)."
    )
    parser.add_argument(
        "--plan-dir",
        default="docs/plan",
        help="Plan directory (default: docs/plan)",
    )
    parser.add_argument(
        "--file",
        default=None,
        help="Only search a specific plan file name (e.g. 04-phase3-typeck.md)",
    )
    parser.add_argument(
        "--contains",
        default=None,
        help="Only match TODO lines containing this substring (case-insensitive).",
    )
    args = parser.parse_args()

    plan_dir = Path(args.plan_dir)
    plan_files = list(iter_plan_files(plan_dir))
    if args.file:
        plan_files = [p for p in plan_files if p.name == args.file]

    for plan_file in plan_files:
        todo = first_unchecked_todo(plan_file, args.contains)
        if todo:
            print(f"{todo.path}:{todo.lineno}: {todo.text}")
            return 0

    return 1


if __name__ == "__main__":
    raise SystemExit(main())

