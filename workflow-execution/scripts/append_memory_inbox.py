#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import datetime
from pathlib import Path

from cli_common import add_dry_run_arg, print_header


def main() -> int:
    parser = argparse.ArgumentParser(description="Append an inbox row into task memory inbox.md")
    parser.add_argument("--file", required=True)
    parser.add_argument("--input", required=True)
    parser.add_argument("--judgement", default="")
    parser.add_argument("--action", default="")
    parser.add_argument("--result", default="")
    parser.add_argument("--promote", default="否")
    parser.add_argument("--time", default=datetime.now().strftime("%H:%M"))
    add_dry_run_arg(parser)
    args = parser.parse_args()

    target = Path(args.file)
    if not target.exists():
        print(f"Error: inbox file not found: {target}")
        return 1

    row = f"| {args.time} | {args.input} | {args.judgement} | {args.action} | {args.result} | {args.promote} |"
    print_header("Append Memory Inbox", {"file": str(target)})
    print(f"- row: {row}")
    if args.dry_run:
        return 0

    lines = target.read_text(encoding="utf-8").splitlines()
    lines.append(row)
    target.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("[PASS] inbox updated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
