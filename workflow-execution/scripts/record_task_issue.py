#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from pathlib import Path

from cli_common import add_dry_run_arg, print_header


def next_issue_id(lines: list[str]) -> str:
    max_num = 0
    for line in lines:
        match = re.search(r"`ISSUE-(\d+)`", line)
        if match:
            max_num = max(max_num, int(match.group(1)))
    return f"ISSUE-{max_num + 1:03d}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Append issue row into task memory issues.md")
    parser.add_argument("--file", required=True)
    parser.add_argument("--issue-id", default="")
    parser.add_argument("--phenomenon", required=True)
    parser.add_argument("--trigger", default="")
    parser.add_argument("--root-cause", default="")
    parser.add_argument("--resolution", default="")
    parser.add_argument("--verification", default="")
    parser.add_argument("--status", default="open")
    parser.add_argument("--note", default="")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    target = Path(args.file)
    if not target.exists():
        print(f"Error: issues file not found: {target}")
        return 1

    lines = target.read_text(encoding="utf-8").splitlines()
    issue_id = args.issue_id or next_issue_id(lines)

    row = (
        f"| `{issue_id}` | {args.phenomenon} | {args.trigger} | {args.root_cause} | "
        f"{args.resolution} | {args.verification} | `{args.status}` | {args.note} |"
    )
    print_header("Record Task Issue", {"file": str(target), "issue_id": issue_id})
    print(f"- row: {row}")
    if args.dry_run:
        return 0

    lines.append(row)
    target.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("[PASS] issue updated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
