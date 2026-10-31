# Java 0.7.2 vs 0.7.5 — protocol and asset comparison

**Status:** investigation complete (2026-10-31)
**Question:** does the release-client version gap (old Java server used for the
Rust rewrite vs the current Java server) explain the Rust server's
`WorldLoadFinished` failure?

## Inputs

| Input | Path | Commit / date |
|---|---|---|
| Old shared source used by the Rust rewrite | `.hytale-source/hytale-shared-source` | `387dc7e0a` — Sync 2026-10-14 |
| Current shared source | `/home/chome/Projects/hytale-shared-source` | `bcc393865` — Sync 2026-10-30 |
| Rust unpacked asset pack | `Assets/extracted` | matches `387dc7e0a` |

The two commits are consecutive release-branch sync points. There are three
commits between them: `e6285f3a4` (2026-10-21), `f063c2a88` (2026-10-22) and
`bcc393865` (2026-10-30).

Reproduce with:

```sh
tools/compare-java-versions.py \
  .hytale-source/hytale-shared-source \
  /home/chome/Projects/hytale-shared-source \
  Assets/extracted
```

## Result 1 — the wire protocol is unchanged

```text
protocol-version.json: old crc=3908406161 build=258; new crc=3908406161 build=258; identical=True
Protocol: files 883 vs 883; changed 0; left-only 0; right-only 0
```

- `Protocol/protocol-version.json` is the same blob in both commits.
- The complete `Protocol/` tree (C# source of truth + generated serializers) has
  **0 changed files** between the two commits.
- `HytaleServer/Protocol` has only four changed Java files, all IO/decoder
  plumbing; no generated packet class changed.
- The Rust generated protocol already asserts the same values:
  `crates/hytale-protocol/tests/registry.rs` checks `settings::CRC ==
  -386_561_135` and `settings::BUILD_NUMBER == 258`.

**Conclusion:** packet IDs, layouts, channels, compression flags and protocol
constants are identical for both Java versions. A 0.7.5 client and a server
using the 0.7.2 protocol are not rejected by the `Connect` CRC/build check.

## Result 2 — Rust uses the old asset pack, not the new one

```text
Rust Server JSON vs old: common 36498; same 36498; different 0;
                         rust-only 6; old-only 0
Rust Server JSON vs new: common 36498; same 36468; different 30;
                         rust-only 6; new-only 4
```

`Assets/extracted/Server` is byte-identical to the 2026-10-14 checkout for
every common JSON file. The six Rust-only files are instance configs, not
assets. The new checkout adds four assets and changes 30:

- 20 `Armor_*_Legs.json` files: add `Overpants` to `CosmeticsToHide`
- 8 Halloween/deco/food item documents: model paths, light colours, new
  `State`/`Interactions` and new pumpkin-lamp/base plush assets
- `Models/Beast/Hatworm.json`: one cosmetic texture path

The full asset tree has 79 changed common files, all cosmetics/event content
(plush models, makeup textures, cosmetology language files, Halloween icons),
plus 4 new Server JSON items. No block/store schema changed.

## Result 3 — Java server asset/packet code did not change

```text
asset code changed in CoreServer: 0
HytaleServer/CoreServer/src/main/java files changed: 18
HytaleAssets files changed: 200
```

The 18 Java source files are Sentry configuration, QUIC/Netty stream handling,
packet handler `handlesPacket` additions and two world commands. Zero files
under `asset/` or `modules/` changed, and no `*PacketGenerator` or `toPacket()`
implementation changed.

One client-facing behaviour is new in 0.7.5: `PacketDecoder` now rejects a
packet before buffering if the current handler does not accept its ID, and
validates that the packet channel matches the stream. The Rust server does not
yet mirror that pre-buffer handler check, but this is a Rust-side hardening
difference, not a protocol-version incompatibility.

## Conclusion for T66

The 0.7.2 → 0.7.5 gap is **not a protocol-version incompatibility**:

- the network contract is byte-identical;
- the Rust asset pack matches the old checkout exactly;
- the only asset deltas are cosmetic/Halloween items and one model;
- no asset-store or packet-generator code changed.

The `WorldLoadFinished` failure is therefore more likely in the Rust server's
runtime behaviour (setup/auth flow, common-asset request handling, config packet
contents or the new strict packet-handler/channel validation) than in the
client/server version pair.

Refreshing `Assets/Assets.zip` to the 0.7.5 asset tree is still useful for
content parity, but it should not be treated as the fix for the connection
failure until a client log identifies a missing 0.7.5 asset or packet.

## Follow-ups

- Capture the release client's exact disconnect/`WorldLoadFinished` log before
  changing protocol or packet generation.
- Consider mirroring 0.7.5's `handlesPacket`/channel pre-checks in the Rust
  transport, since that is the only new client-facing server validation.
- Refresh the asset pack separately, with the 30 changed and 4 added Server
  assets re-baselined through the existing asset tests.
