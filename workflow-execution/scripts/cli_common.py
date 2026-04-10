#!/usr/bin/env python3
"""Thin CLI common layer for workflow-execution scripts.

Localized copy to keep the skill self-contained without relying on _shared/.
"""
from __future__ import annotations

import argparse
from typing import Any

from load_profile import get_profile


def add_profile_arg(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--profile",
        default=None,
        help="Path to project-profile.yml. Auto-detected if omitted.",
    )


def add_dry_run_arg(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print planned actions without writing files or running commands.",
    )


def load_profile_from_args(args: argparse.Namespace) -> dict[str, Any]:
    return get_profile(getattr(args, "profile", None))


def print_header(title: str, fields: dict[str, str | None] | None = None) -> None:
    print(f"# {title}")
    if fields:
        for key, value in fields.items():
            if value is not None:
                print(f"- {key}: {value}")
