#!/usr/bin/env python3
"""Summarize a `HYTALE_PLACEMENT_TRACE` JSONL file.

The server writes one record per placement when `HYTALE_PLACEMENT_TRACE` is
set. This tool groups the records by block type and prints the observed
rotation tuples, so a release-client run can establish the client's
`RandomRotation` selection. It does not assume a selection algorithm.
"""

from __future__ import annotations

import json
import sys
from collections import defaultdict
from pathlib import Path


def rotation_name(index: int) -> str:
    yaw = index % 4
    pitch = (index // 4) % 4
    roll = (index // 16) % 4
    return f"yaw={yaw * 90} pitch={pitch * 90} roll={roll * 90}"


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {Path(sys.argv[0]).name} <placements.jsonl>", file=sys.stderr)
        return 2

    groups: dict[str, list[dict]] = defaultdict(list)
    for line_number, line in enumerate(Path(sys.argv[1]).read_text().splitlines(), 1):
        line = line.strip()
        if not line:
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            print(f"{sys.argv[1]}:{line_number}: {error}", file=sys.stderr)
            return 1
        if not record.get("client_rotation", False):
            continue
        groups[record.get("block_key", "<unknown>")].append(record)

    if not groups:
        print("no client-authored placement records", file=sys.stderr)
        return 1

    for block_key in sorted(groups):
        records = groups[block_key]
        random_rotation = records[0].get("random_rotation", "?")
        print(f"{block_key} ({random_rotation}, {len(records)} placements)")
        for record in sorted(records, key=lambda item: (item["x"], item["y"], item["z"])):
            rotation = int(record["final_rotation"])
            print(
                f"  ({record['x']:>4}, {record['y']:>3}, {record['z']:>4}) "
                f"tuple={rotation:>2} {rotation_name(rotation)} "
                f"requested={record['requested_rotation']}"
            )
        tuples = sorted({int(record["final_rotation"]) for record in records})
        print(f"  observed tuples: {tuples}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
