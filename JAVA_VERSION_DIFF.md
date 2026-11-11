# Java shared-source delta — 2026-10-14 → 2026-11-05

**Status:** refreshed 2026-11-11
**Question:** does the newer Java shared source change the protocol, assets or
runtime behavior that the Rust server ports?

## Inputs

| Input | Path | Commit | Date |
|---|---|---|---|
| Archived reference (Rust rewrite + goldens) | `.hytale-source/hytale-shared-source` | `387dc7e0a` | 2026-10-14 |
| Previously compared tree | `~/Projects/hytale-shared-source` (before pull) | `bcc393865` | 2026-10-30 |
| Current pulled tree | `~/Projects/hytale-shared-source` | `b9eb29b30` | 2026-11-05 |
| Rust asset pack | `Assets/extracted` | matches `387dc7e0a` | — |

The pulled tree contains three syncs after the archived reference:
`e6285f3a4` (2026-10-21), `f063c2a88` (2026-10-22) and `bcc393865`
(2026-10-30), plus `b9eb29b30` (2026-11-05).

Reproduce with:

```sh
tools/compare-java-versions.py \
  .hytale-source/hytale-shared-source \
  ~/Projects/hytale-shared-source \
  Assets/extracted
```

## Result 1 — the wire protocol is unchanged

```text
protocol-version.json: old crc=3908406161 build=258; new crc=3908406161 build=258; identical=True
Protocol: files 883 vs 883; changed 0; left-only 0; right-only 0
```

- Packet IDs, layouts, channels, compression flags and protocol constants are
  identical.
- The only `HytaleServer/Protocol` edits are IO plumbing: the new
  `ConnectionHandler.handlesPacket` hook and `PacketDecoder`'s pre-buffer
  channel/handler checks (see Result 3). No generated packet class changed.
- The Rust generated protocol still asserts the same values in
  `crates/hytale-protocol/tests/registry.rs` (`CRC == -386_561_135`,
  `BUILD_NUMBER == 258`).

**Conclusion:** a 0.7.x client and the Rust server still agree on the network
contract. No protocol codegen change is required.

## Result 2 — Rust assets still match the archived pack

```text
Rust Server JSON vs old: common 36498; same 36498; different 0; rust-only 6; old-only 0
Rust Server JSON vs new: common 36498; same 36468; different 30; rust-only 6; new-only 4
Server JSON old -> new: changed 30; added 4
```

`Assets/extracted/Server` is still byte-identical to the 2026-10-14 checkout.
The newer tree changes 30 and adds 4 Server documents:

- **20 `Armor_*_Legs.json`** add `Overpants` to `CosmeticsToHide`. Rust already
  maps `Cosmetic::Overpants`
  (`crates/hytale-assets/src/packets/item.rs`).
- **8 Halloween/deco/food items** retarget `CustomModel`/texture paths from
  `Items/Halloween_Props/**` to `Blocks/Decorative_Sets/Halloween/**`, adjust
  light colours, and add `State.Definitions` plus `Use`/`ChangeState`
  interactions. Rust already loads `State.Definitions` and executes
  `ChangeState` (`crates/hytale-assets/src/packets/block_type.rs`,
  `crates/hytale-interactions/src/executor.rs`).
- **`Models/Beast/Hatworm.json`** swaps one cosmetic texture path.
- **`Server/Languages/en-US/server.lang`** adds names for the new items.
- **4 new items**: `Deco_Bat_Plush` and three
  `Deco_Halloween_Pumpkin_Lamp_*` variants, with new Common models, icons and
  cosmetics. The new items use the existing item/block/interaction shapes.

No block, item or store schema changed, so the
`--validate-assets` store list and count are unchanged. The local pack still
has 3,960 item documents; the newer tree has 3,964.

**Consequence:** the running server needs no code change for the new content.
If the asset pack is refreshed, the item/icon goldens (the 3,971-item packet
assertions) and any content-derived fixtures must be re-baselined with
`tools/java-dumper`; the Rust parsers should accept the new documents as-is.

## Result 3 — server code deltas and Rust impact

The 260 changed files are mostly content (155 Common assets/cosmetics, 10
Cosmetics documents, 35 Server assets). The `HytaleServer` source changes fall
into two groups.

### Relevant to the Rust server

| Java change | Rust state | Impact |
|---|---|---|
| New `security/CertificateTrustPolicy` deny-lists eight compromised eMudhra intermediate CA SPKIs for outbound HTTPS (`Main.java` installs it) | `hytale-auth` now uses a blocking rustls client whose verifier rejects the same eight SPKI hashes (`crates/hytale-auth/src/https.rs`) | Closed by **T68** |
| `ConnectionHandler.handlesPacket`, `PacketDecoder` channel/handler pre-checks, `QuicheChannel.acceptsPacket`/`checkHandled` reject wrong-channel or wrong-stage packets before buffering | `FrameReader`/`ChunkStreamReader` and the chunk/world-map writers enforce the same channel policy before payload allocation; the session keeps its stage checks | Closed by **T68** |
| `QuicheListener` lowers the non-loopback QUIC payload ceiling to 1452 bytes, uses 5000 on loopback and a 1200 floor for IPv4/private peers; `QuicheConnection` sends paced datagrams | server/client endpoints use `quinn::EndpointConfig::max_udp_payload_size(1452)` with MTU discovery enabled | Closed by **T68** (loopback 5000 parity remains a perf follow-up) |
| JWKS is now committed under `CoreServer/src/main/jwks/<env>.json` instead of fetched at build time | `hytale-auth` fetches `/well-known/jwks.json` at runtime and caches `.jwks.json` | No break; offline-JWKS parity remains a documented follow-up |
| `JWTValidator` comment only | — | None |

### Not applicable to the Rust server

- `SentryDsn` config, `ThirdPartySentryFilter` and the plugin/event Sentry
  routing changes: the Rust server has no Sentry and its plugins are compiled
  in.
- Singleplayer `localhost` binding: Rust already binds explicit loopback
  addresses (`ServerEndpoint::bind_addrs`).
- `HytaleGenerator` density-`anchor` unification, `OffsetX/Y/Z` int→double and
  thread-count tuning: `builtin/HytaleGenerator` remains a separate engine
  project (T61); Rust worldgen does not port that asset graph.
- Beds respawn-owner/state-save fix and chunk-tint updates on chunk
  regeneration: Rust has no respawn-point pages, regeneration command or tint
  notifications.
- `CosmeticsModule` disconnect-message format note: Rust does not emit
  `invalidSkin` validation messages.

## Conclusion for T66 (preserved)

The 0.7.2 → 0.7.5 gap was not a protocol-version incompatibility: the network
contract is byte-identical, the Rust pack matches the old checkout exactly, and
the only asset deltas were cosmetic/Halloween content. The `WorldLoadFinished`
failure was therefore a runtime-flow problem, not a version pair, and T66 fixed
it on the Rust side.

## Current conclusion (2026-11-11)

The 2026-11-05 shared source does **not** break the Rust server: the protocol is
unchanged and the asset deltas are content-only. The remaining differences —
outbound TLS hardening, packet channel validation and QUIC payload tuning — were
closed by [T68](tasks/T68-upstream-transport-auth-hardening.md) in session 95.

## Follow-ups

- Loopback QUIC throughput parity (Java 5000 vs quinn's endpoint-wide 1452) if
  benchmarks show it matters.
- Refresh `Assets/Assets.zip`/`Assets/extracted` to the 2026-11-05 tree in a
  separate change, then re-baseline the item/icon goldens with
  `tools/java-dumper`.
- Consider bundling the release JWKS for offline live-auth validation when the
  credential-persistence follow-up lands.
