#!/usr/bin/env python3
"""Report owner lines, since the v4.0.14 merge-base, missing from a merge result.

Default: HEAD is the merge of tag v4.0.17, the first parent is the fork tip,
and the merge tree is the result. During an uncommitted merge, HEAD is still
the fork tip and the worktree is the result.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


BASE = "v4.0.16"
THEIRS = "v4.0.17"


def git(*args: str) -> str:
    out = subprocess.run(
        ["git", *args],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return out.stdout


def git_ok(*args: str) -> bool:
    proc = subprocess.run(
        ["git", *args],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return proc.returncode == 0


def names(a: str, b: str) -> set[str]:
    text = git("diff", "--name-only", a, b)
    return {line for line in text.splitlines() if line}


def blob(rev: str, path: str) -> list[str] | None:
    proc = subprocess.run(
        ["git", "show", f"{rev}:{path}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if proc.returncode != 0:
        return None
    return proc.stdout.splitlines()


def interesting(line: str) -> bool:
    stripped = line.strip()
    return len(stripped) >= 8


def resolve() -> tuple[str, str | None]:
    """Return (ours_rev, result_rev). result_rev None means read the worktree."""
    parents = git("rev-list", "--parents", "-n", "1", "HEAD").split()
    merging = Path(".git/MERGE_HEAD").exists()
    if merging or len(parents) < 3:
        return "HEAD", None
    if git_ok("merge-base", "--is-ancestor", THEIRS, "HEAD"):
        return parents[1], "HEAD"
    return "HEAD", None


def main() -> int:
    ours, result_rev = resolve()
    both = sorted(names(BASE, ours) & names(BASE, THEIRS))
    ours_only = sorted(names(BASE, ours) - names(BASE, THEIRS))
    touched = []
    for path in ours_only:
        if result_rev is None:
            changed = not git_ok("diff", "--quiet", ours, "--", path)
        else:
            changed = not git_ok("diff", "--quiet", ours, result_rev, "--", path)
        if changed:
            touched.append(path)

    print(f"ours {ours}")
    print(f"result {result_rev or 'worktree'}")
    print(f"both-sides {len(both)}")
    print(f"owner-only {len(ours_only)}")
    print(f"owner-only touched by merge {len(touched)}")
    for path in touched:
        print(f"TOUCHED {path}")

    missing_files = 0
    missing_lines = 0
    for path in both:
        base_lines = blob(BASE, path) or []
        ours_lines = blob(ours, path)
        theirs_lines = blob(THEIRS, path) or []
        if result_rev is None:
            file = Path(path)
            result_lines = (
                file.read_text(errors="replace").splitlines() if file.exists() else None
            )
        else:
            result_lines = blob(result_rev, path)
        if ours_lines is None:
            continue
        if result_lines is None:
            print(f"DELETED {path}")
            missing_files += 1
            continue
        base_set = set(base_lines)
        result_set = set(result_lines)
        theirs_set = set(theirs_lines)
        dropped = []
        seen = set()
        for line in ours_lines:
            if line in base_set or line in result_set or line in seen:
                continue
            if not interesting(line):
                continue
            seen.add(line)
            where = "also-upstream" if line in theirs_set else "fork-only"
            dropped.append((where, line.strip()))
        if not dropped:
            continue
        missing_files += 1
        missing_lines += len(dropped)
        print(f"\nFILE {path} dropped {len(dropped)}")
        for where, line in dropped[:40]:
            print(f"  {where}: {line[:220]}")
        if len(dropped) > 40:
            print(f"  ... {len(dropped) - 40} more")

    print(f"\nsummary files={missing_files} lines={missing_lines}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
