#!/usr/bin/env python3
"""Compare two Hytale shared-source checkouts and the Rust asset pack.

Usage:
    tools/compare-java-versions.py OLD_ROOT NEW_ROOT [RUST_ASSET_ROOT]

OLD_ROOT and NEW_ROOT are Hytale shared-source checkouts. RUST_ASSET_ROOT is
the unpacked Rust asset root (default: Assets/extracted).

The script answers the version-mismatch question directly:
- does the shared protocol version differ?
- which protocol files differ?
- do the Rust server assets match the old checkout or the new one?
- which Server JSON assets changed between the two checkouts?
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree_hashes(root: Path, suffix: str | None = None) -> dict[str, str]:
    files = (p for p in root.rglob("*") if p.is_file())
    if suffix is not None:
        files = (p for p in files if p.suffix == suffix)
    return {str(p.relative_to(root)): sha256(p) for p in files}


def compare_trees(label: str, left: Path, right: Path) -> None:
    lfiles = tree_hashes(left)
    rfiles = tree_hashes(right)
    common = set(lfiles) & set(rfiles)
    changed = sorted(path for path in common if lfiles[path] != rfiles[path])
    left_only = sorted(set(lfiles) - set(rfiles))
    right_only = sorted(set(rfiles) - set(lfiles))
    print(
        f"{label}: files {len(lfiles)} vs {len(rfiles)}; "
        f"changed {len(changed)}; left-only {len(left_only)}; right-only {len(right_only)}"
    )
    for path in changed[:20]:
        print(f"  changed {path}")
    if len(changed) > 20:
        print(f"  ... {len(changed) - 20} more changed")
    for path in left_only[:10]:
        print(f"  left-only {path}")
    for path in right_only[:10]:
        print(f"  right-only {path}")


def main(argv: list[str]) -> int:
    if len(argv) not in (3, 4):
        print(__doc__, file=sys.stderr)
        return 2
    old_root = Path(argv[1]).resolve()
    new_root = Path(argv[2]).resolve()
    rust_assets = Path(argv[3] if len(argv) == 4 else "Assets/extracted").resolve()

    old_version = old_root / "Protocol/protocol-version.json"
    new_version = new_root / "Protocol/protocol-version.json"
    old_data = json.loads(old_version.read_text())
    new_data = json.loads(new_version.read_text())
    print(
        "protocol-version.json: "
        f"old crc={old_data.get('crc')} build={old_data.get('buildNumber')}; "
        f"new crc={new_data.get('crc')} build={new_data.get('buildNumber')}; "
        f"identical={old_data == new_data}"
    )

    compare_trees("Protocol", old_root / "Protocol", new_root / "Protocol")

    old_assets = old_root / "HytaleAssets/Server"
    new_assets = new_root / "HytaleAssets/Server"
    rust_server = rust_assets / "Server"
    old_hashes = tree_hashes(old_assets, ".json")
    new_hashes = tree_hashes(new_assets, ".json")
    rust_hashes = tree_hashes(rust_server, ".json")

    for label, other in (("old", old_hashes), ("new", new_hashes)):
        common = set(rust_hashes) & set(other)
        same = sum(1 for path in common if rust_hashes[path] == other[path])
        print(
            f"Rust Server JSON vs {label}: common {len(common)}; "
            f"same {same}; different {len(common) - same}; "
            f"rust-only {len(set(rust_hashes) - set(other))}; "
            f"{label}-only {len(set(other) - set(rust_hashes))}"
        )

    changed = sorted(
        path
        for path in set(old_hashes) & set(new_hashes)
        if old_hashes[path] != new_hashes[path]
    )
    added = sorted(set(new_hashes) - set(old_hashes))
    print(f"Server JSON old -> new: changed {len(changed)}; added {len(added)}")
    for path in changed:
        print(f"  changed {path}")
    for path in added:
        print(f"  added {path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
