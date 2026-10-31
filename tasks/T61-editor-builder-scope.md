# T61 — Editor, builder tools, machinima, voice and cosmetics scope

**Status:** done (session 66)
**Package:** docs, `hytale-server`, `tools/client-bot`
**Depends on:** T60
**Agent fit:** low

## Goal

Decide what the Rust CoreServer supports from the post-M6 feature set — the
editor and AssetEditor, builder tools, machinima, voice and cosmetics — without
turning parity work into client-authoring or media-infrastructure work. Keep
`builtin/HytaleGenerator` as a separate engine project.

This is a scope decision, not a broad implementation. Only the slices marked
**in scope** below were implemented, and each has a regression test.

## Evidence

- The protocol crate already has complete generated codecs for the deferred
  families: 35 `BuilderTool*` packet/arg/action types, the three machinima
  packets (`RequestMachinimaActorModel`, `SetMachinimaActorModel`,
  `UpdateMachinimaScene`), the three voice packets (`VoiceData`,
  `RelayedVoiceData`, `VoiceConfig`) plus the `Voice` channel, and 58
  `AssetEditor*` types plus `EditorBlocksChange`, `EditorSelection` and the
  editor time/weather override packets. Missing server gameplay is therefore a
  semantics decision, not a wire-format gap.
- Cosmetics are already carried as appearance data: the live identity token's
  `profile.skin` and the saved `PlayerSkin` component map all 20 `PlayerSkin`
  slots, and `Cosmetic`/`ItemArmor.cosmeticsToHide` stay encoded by the item
  asset packet. The character-creator catalogs
  (`Assets/extracted/Cosmetics/CharacterCreator`, 29 JSON documents) and the
  1,236 cosmetic models/textures are client-side presentation assets.
- Builder-tool item data (`builderToolData`, `blockSelectorTool`, `tool`,
  `weapon`, `armor`, `gliderConfig`, `trails`, `itemAppearanceConditions`,
  `hudUI`, `ability`) is still `None` in the item packet. That is
  config-asset completeness and belongs to
  [T66](T66-live-client-assets.md), not T61.
- `Assets/extracted/Server/HytaleGenerator` (351 files) is the
  density/assignment world generator. It is not the classic
  `WorldGenerator` port in `worldgen-hytale` and has no server runtime
  integration.
- Editor `Connect` was refused only in local/authenticated auth; insecure mode
  accepted `ClientType::Editor` even though T65 recorded editor clients as
  unsupported.

## Decision

| Area | In scope | Out of scope | Why |
|---|---|---|---|
| Editor / AssetEditor | Refuse every editor connection before auth/setup | Editor world loading, editor permissions, block/selection sync, time/weather overrides, asset authoring/file operations | The Rust server is a game server. The AssetEditor is an authoring tool with 58 request/reply types and pack/filesystem semantics. |
| Builder tools | Existing generic client-packet ignore only; keep player block place/break | Creative brushes, clipboard/copy-paste, undo/redo, entity transforms, masks, prefab preview, selection transforms | These need a creative-mode world-edit model that M5/M6 does not define. Builder item data stays with T66. |
| Machinima | Nothing | Scene recording/playback, actor model requests/updates, scene synchronization | Client-driven presentation; no server gameplay state to own. |
| Voice | Nothing; never relay or mix voice | Voice channel sessions, Opus relay, positional/underwater state, muting, codec negotiation | Social/media infrastructure with no gameplay parity requirement in v1. |
| Cosmetics | Appearance passthrough from live auth and saved `PlayerSkin`; all 20 slots for self and remote players | Character-creator catalogs/UI, entitlement/unlock checks, cosmetic shop/economy, cosmetic hiding rules beyond the item payload | The server only needs to authenticate and replicate the selected appearance. Catalogs are client assets. |
| HytaleGenerator | Nothing in this repository | Density/assignment generator runtime and its 351 assets | Separate engine project; `worldgen-hytale` remains the classic `WorldGenerator` port. |

## Approved parity slice

1. **Editor clients are refused in every auth mode.** The `ClientType::Editor`
   check moved ahead of the `AuthMode` match in
   `crates/hytale-server/src/session.rs`, so insecure mode can no longer accept
   an editor connection. The server sends the existing
   `ServerDisconnect("editor connections are not supported")` and returns
   `SessionError::UnexpectedPacket { state: "auth:editor" }`.
2. **Cosmetic appearance passthrough is regression-tested.** The live-token
   decoder is checked against all 20 `PlayerSkin` fields (including the nested
   JSON-string claim form and malformed input), and the saved-skin path is
   checked through the spawned `EntityUpdates` packet.

No builder-tool, machinima or voice semantics were implemented. Those packets
continue to fall through the generic client-packet ignore path; they do not
mutate world state or disconnect the session.

## Acceptance

```sh
cargo test -p hytale-server --lib cosmetic
cargo test -p client-bot --test editor_rejection -- --ignored
./tools/test.sh fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- An editor `Connect` gets a `ServerDisconnect` and is rejected before the
  insecure options / auth exchange.
- Every `PlayerSkin` cosmetic slot survives auth-token decoding and the saved
  skin path.
- The scope table is the source of truth for T61 follow-up requests.

## Deviations

- The editor rejection is a behavior change for insecure-mode editor clients.
  They previously reached `RequestInsecurePlayerOptions`; they are now refused
  consistently with local/authenticated mode.
- The E2E editor test is `#[ignore]`d because it needs the base-game pack and
  loopback UDP, like the other client-bot loopback tests.
- Builder-tool item payload fields remain empty until T66 lands the item
  config-asset completeness work.

## Follow-ups

- If creative/builder editing becomes a product requirement, start a new task
  with a world-edit transaction model (selection, clipboard, undo) instead of
  wiring the protocol packets directly.
- Voice relay and machinima scene synchronization require separate transport
  and lifecycle designs; they are not T61 follow-ups.
- `builtin/HytaleGenerator` can become its own engine project once the classic
  `worldgen-hytale` port is no longer the v1 target.
