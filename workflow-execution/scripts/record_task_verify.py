#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import datetime
from pathlib import Path

from cli_common import add_dry_run_arg, print_header


def main() -> int:
    parser = argparse.ArgumentParser(description="Append verify record into task memory verify.md")
    parser.add_argument("--file", required=True)
    parser.add_argument("--action", required=True, help="Verification command or action")
    parser.add_argument("--result", required=True)
    parser.add_argument("--coverage", default="")
    parser.add_argument("--risk", default="")
    parser.add_argument("--time", default=datetime.now().strftime("%Y-%m-%d %H:%M"))
    add_dry_run_arg(parser)
    args = parser.parse_args()

    target = Path(args.file)
    if not target.exists():
        print(f"Error: verify file not found: {target}")
        return 1

    row = f"| {args.time} | {args.action} | {args.result} | {args.coverage} | {args.risk} |"
    print_header("Record Task Verify", {"file": str(target)})
    print(f"- row: {row}")
    if args.dry_run:
        return 0

    lines = target.read_text(encoding="utf-8").splitlines()
    lines.append(row)
    target.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("[PASS] verify updated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
