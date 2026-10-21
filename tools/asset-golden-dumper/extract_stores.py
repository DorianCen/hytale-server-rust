#!/usr/bin/env python3
"""Extract the 54 Hytale asset store registrations into JSON and Markdown.

The source of truth is `AssetRegistryLoader.java`. This script only reads it;
it never edits the Java tree. Run from anywhere:

    python3 tools/asset-golden-dumper/extract_stores.py

Outputs are deterministic and committed:
    tools/asset-golden-dumper/asset-stores.json
    tools/asset-golden-dumper/asset-stores.md
"""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOADER = (
    ROOT
    / ".hytale-source/hytale-shared-source/HytaleServer/CoreServer/src/main/java"
    / "com/hypixel/hytale/server/core/asset/AssetRegistryLoader.java"
)
SCHEMA_DIR = ROOT / ".hytale-source/hytale-shared-source/HytaleAssets/Schema"
OUT_JSON = Path(__file__).with_name("asset-stores.json")
OUT_MD = Path(__file__).with_name("asset-stores.md")

P0 = {
    "BlockType",
    "Fluid",
    "Environment",
    "Item",
    "BlockBoundingBoxes",
    "PhysicalMaterial",
    "BlockTypeListAsset",
    "PrefabListAsset",
}
P1 = {
    "BlockSet",
    "BlockSoundSet",
    "BlockParticleSet",
    "BlockBreakingDecal",
    "ItemCategory",
    "ItemQuality",
    "ModelAsset",
    "GameplayConfig",
}
P2 = {
    # audio
    "MusicContainer",
    "AmbienceFX",
    "ItemSoundSet",
    "SoundEvent",
    "SoundSet",
    "AudioState",
    "Stinger",
    "MusicTransition",
    "AudioCategory",
    "ReverbEffect",
    "EqualizerEffect",
    # particles / trails / VFX
    "ParticleSpawner",
    "ParticleSystem",
    "Trail",
    "FluidFX",
    "EntityEffect",
    "ModelVFX",
    # weather / camera / response curves / tags
    "Weather",
    "CameraEffect",
    "ResponseCurve",
    "TagPattern",
}

WORLDGEN = {"BlockType", "Fluid", "Environment", "Item", "BlockBoundingBoxes"}
EMBEDDED = {
    "BlockType": "BlockType data is also embedded in Item assets via ContainedAssetCodec.",
    "BlockBoundingBoxes": "Referenced by BlockType; also used by worldgen collision.",
    "FluidFX": "Referenced by Fluid and Environment.",
}


def match_paren(text: str, start: int) -> int:
    """Return the index just after the parenthesis that closes at `start`."""
    depth = 0
    for index in range(start, len(text)):
        char = text[index]
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return index + 1
    raise ValueError("unbalanced parenthesis")


def split_top_level(text: str) -> list[str]:
    parts: list[str] = []
    depth = 0
    start = 0
    for index, char in enumerate(text):
        if char in "(<[":
            depth += 1
        elif char in ")>]":
            depth -= 1
        elif char == "," and depth == 0:
            parts.append(text[start:index].strip())
            start = index + 1
    tail = text[start:].strip()
    if tail:
        parts.append(tail)
    return parts


def call_argument(block: str, method: str) -> str | None:
    marker = f".{method}("
    start = block.find(marker)
    if start < 0:
        return None
    open_paren = start + len(marker) - 1
    close = match_paren(block, open_paren)
    return block[open_paren + 1 : close - 1].strip()


def class_names(expression: str | None) -> list[str]:
    if not expression:
        return []
    return [
        match.group(1)
        for match in re.finditer(r"\b([A-Za-z_][A-Za-z0-9_]*)\.class\b", expression)
    ]


def preloads(expression: str | None) -> list[str]:
    if not expression:
        return []
    names = re.findall(r"\b[A-Za-z_][A-Za-z0-9_]*\.([A-Z][A-Z0-9_]*)\b", expression)
    return names


def priority_for(name: str) -> str:
    if name in P0:
        return "P0"
    if name in P1:
        return "P1"
    if name in P2:
        return "P2"
    return "P3"


def notes_for(store: dict) -> str:
    notes: list[str] = []
    name = store["class"]
    if name in EMBEDDED:
        notes.append(EMBEDDED[name])
    if name in WORLDGEN:
        notes.append("Required by world generation.")
    if store["schema"] == "none":
        notes.append("No matching JSON schema in HytaleAssets/Schema.")
    if store["packetGenerator"] == "none":
        notes.append("No packet generator; server-internal only.")
    if store["key"] != "String":
        notes.append(f"Keyed by {store['key']}.")
    if name == "BlockType":
        notes.append("Id assignment is owned by BlockTypeAssetMap.")
    if name == "Item":
        notes.append("Preloaded with the unknown item.")
    if name == "AudioCategory":
        notes.append("Preload order is AudioCategory.PRELOAD_ORDER.")
    if name == "AmbienceFX":
        notes.append("Depends on most audio and environment stores.")
    return " ".join(notes)


def main() -> None:
    text = LOADER.read_text()
    schemas = {path.name for path in SCHEMA_DIR.glob("*.json")}
    map_variables = dict(
        re.findall(r"var\s+(\w+)\s*=\s*new\s+(\w+)", text)
    )

    stores: list[dict] = []
    cursor = 0
    marker = "HytaleAssetStore.builder("
    while True:
        start = text.find(marker, cursor)
        if start < 0:
            break
        open_paren = start + len(marker) - 1
        after_args = match_paren(text, open_paren)
        args = text[open_paren + 1 : after_args - 1]

        build = text.find(".build()", after_args)
        if build < 0:
            raise ValueError("builder without .build()")
        block = text[after_args:build]

        parts = split_top_level(args)
        classes = [
            part[: -len(".class")].strip()
            for part in parts
            if part.endswith(".class")
        ]
        if len(classes) >= 2:
            key, name = classes[0], classes[1]
        else:
            key, name = "String", classes[0]

        map_expression = next(
            (part for part in parts if "AssetMap" in part),
            "",
        )
        map_match = re.search(r"\bnew\s+(\w*AssetMap)\b", map_expression)
        if map_match:
            map_name = map_match.group(1)
        elif map_expression in map_variables:
            map_name = map_variables[map_expression]
        else:
            map_name = "DefaultAssetMap"

        packet = call_argument(block, "setPacketGenerator")
        packet_match = re.search(r"\bnew\s+(\w+)\s*\(", packet or "")
        packet_name = packet_match.group(1) if packet_match else "none"

        codec = call_argument(block, "setCodec") or ""
        codec_name = codec.split(".")[0] if codec else "none"

        store = {
            "class": name,
            "path": (call_argument(block, "setPath") or "").strip('"'),
            "key": key,
            "map": map_name,
            "codec": codec_name,
            "packetGenerator": packet_name,
            "loadsBefore": class_names(call_argument(block, "loadsBefore")),
            "loadsAfter": class_names(call_argument(block, "loadsAfter")),
            "preLoadAssets": preloads(call_argument(block, "preLoadAssets")),
            "schema": f"{name}.json" if f"{name}.json" in schemas else "none",
            "priority": priority_for(name),
            "extension": (call_argument(block, "setExtension") or "").strip('"') or "none",
        }
        store["notes"] = notes_for(store)
        stores.append(store)
        cursor = build + len(".build()")

    stores.sort(key=lambda store: (store["priority"], store["class"].lower()))
    OUT_JSON.write_text(json.dumps(stores, indent=2) + "\n")

    counts = {priority: 0 for priority in ("P0", "P1", "P2", "P3")}
    packets = 0
    for store in stores:
        counts[store["priority"]] += 1
        if store["packetGenerator"] != "none":
            packets += 1

    lines = [
        "# Hytale asset stores",
        "",
        "Generated by `extract_stores.py` from `AssetRegistryLoader.java`. Do not edit by hand.",
        "",
        f"- Registrations: **{len(stores)}**",
        f"- With a packet generator: **{packets}**",
        "- Priority counts: "
        + ", ".join(f"{key}={value}" for key, value in counts.items()),
        "",
        "Priority follows `plan.md` §5.2: P0 is join/worldgen critical,",
        "P1 is join-adjacent asset packets, P2 is full asset streaming, and P3",
        "is parity/deferred UI/editor/gameplay data.",
    ]
    for priority in ("P0", "P1", "P2", "P3"):
        lines.extend(
            [
                "",
                f"## {priority}",
                "",
                "| Store | Path | Key | Map | Schema | Packet | Notes |",
                "|---|---|---|---|---|---|---|",
            ]
        )
        for store in stores:
            if store["priority"] != priority:
                continue
            notes = store["notes"].replace("|", "\\|")
            lines.append(
                "| `{class_}` | `{path}` | {key} | {map} | `{schema}` | `{packet}` | {notes} |".format(
                    class_=store["class"],
                    path=store["path"],
                    key=store["key"],
                    map=store["map"],
                    schema=store["schema"],
                    packet=store["packetGenerator"],
                    notes=notes,
                )
            )
    OUT_MD.write_text("\n".join(lines) + "\n")
    print(f"wrote {len(stores)} stores ({packets} packet generators) to {OUT_JSON.name} and {OUT_MD.name}")


if __name__ == "__main__":
    main()
