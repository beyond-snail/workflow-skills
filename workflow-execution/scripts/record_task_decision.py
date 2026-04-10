#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import datetime
import re
from pathlib import Path

from cli_common import add_dry_run_arg, print_header


def next_decision_id(lines: list[str]) -> str:
    max_num = 0
    for line in lines:
        match = re.search(r"`DEC-(\d+)`", line)
        if match:
            max_num = max(max_num, int(match.group(1)))
    return f"DEC-{max_num + 1:03d}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Append decision row into task memory decisions.md")
    parser.add_argument("--file", required=True)
    parser.add_argument("--decision-id", default="")
    parser.add_argument("--decision", required=True)
    parser.add_argument("--reason", default="")
    parser.add_argument("--rejected", default="")
    parser.add_argument("--risk", default="")
    parser.add_argument("--evidence", default="")
    parser.add_argument("--time", default=datetime.now().strftime("%Y-%m-%d %H:%M"))
    add_dry_run_arg(parser)
    args = parser.parse_args()

    target = Path(args.file)
    if not target.exists():
        print(f"Error: decisions file not found: {target}")
        return 1

    lines = target.read_text(encoding="utf-8").splitlines()
    decision_id = args.decision_id or next_decision_id(lines)

    row = (
        f"| `{decision_id}` | {args.time} | {args.decision} | {args.reason} | "
        f"{args.rejected} | {args.risk} | {args.evidence} |"
    )
    print_header("Record Task Decision", {"file": str(target), "decision_id": decision_id})
    print(f"- row: {row}")
    if args.dry_run:
        return 0

    lines.append(row)
    target.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("[PASS] decision updated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
