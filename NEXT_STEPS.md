# Next steps — CoreServer rewrite, phase 2 (session 30)

**Status:** the first vertical slice is archived in
[`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md)
(tasks T01–T33, sessions 1–23). The phase-2 plan is [`plan.md`](plan.md) and
the task board is [`tasks/README.md`](tasks/README.md).

**Next task:** **T43** — world clock and `GameplayConfig`, the start of M4's v1
client path.

## Current state

Landed and green:

- Protocol codegen, framing, QUIC/mTLS transport.
- P0 asset registry + deterministic ids; config packets `41`, `87`, `83`, `61`,
  `40`, `54`.
- T35 child-store goldens (26 stores, 21 payloads) with selector/closure support
  in `AssetDumper`; `BlockSet` and `GameplayConfig` gaps are recorded.
- T36 block/fluid child stores (`BlockBreakingDecal`, `BlockParticleSet`,
  `BlockSoundSet`, `FluidFX`, `SoundEvent` + audio deps); all five payloads are
  byte-identical to Java and `BlockType`/`Fluid`/`Environment` child indexes
  resolve.
- T37 item child stores (`ItemSoundSet`, `ItemQuality`, `ItemReticleConfig`,
  `ItemCategory`, `ItemPlayerAnimations`); all five payloads are byte-identical
  and the item quality/entity/sound/reticle/animation fields have zero residual
  differences over all 3,971 Java items.
- T38 `TagPattern` (196 assets, exact Java key/id parity) and
  `UpdateTagPatterns`; pattern tags join the deterministic `TagRegistry`.
- T39 final config order (17 packets), one encoded `ConfigAssets` shared by all
  sessions, `--validate-assets`, and client-bot packet recognition.
- T40 `hytale-core`: typed layered config, text/JSON logging init, async task
  scheduler with a shutdown deadline, ordered sync/async event bus, and the
  module-manifest registry seam.
- T41 universe/world lifecycle: persistent catalog (`universe.json`), name/UUID
  resolution, default world, create/load/save/delete, per-world
  `UniverseRuntime` threads, player load/save seam, and the `universe` CLI.
  Flat/Void/Dummy providers are empty-column stubs until M5.
- T42 persistent `serve` listener: one process-level universe/world thread,
  independent session tasks over the existing join state machine, shared cached
  config burst, connection limit, idle timeout, clean shutdown and the
  two-client disconnect/rejoin loopback test.
- Runtime chunks, BSON chunk/world/player storage, world config, save boot,
  demand loading, world thread and Java-ordered chunk packets.
- Dev/insecure and local-EdDSA authenticated join; common/config asset
  streaming; Game transition; player list/self-spawn; time/gameplay packets;
  teleport exchange; persistent chunk stream and movement-driven view updates.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Phase 2 sequence

1. **M2 — asset completion (done):** T35 → T36/T37/T38 → T39.
2. **M3 — runtime:** T40 (done) → T41 (done) → T42 (done; parallel with M2 after T34).
3. **M4 — v1:** T43 → T44 → T45 → T46 → T47 → T48.
4. **M5/M6/M7:** gameplay, builtins/NPC, hardening (see [`plan.md`](plan.md)).

## Try it

The phase-1 commands still work:

```sh
cargo run -p hytale-server -- --validate-assets
cargo run -p hytale-server -- universe --root target/universe create Alpha --seed 1
cargo run -p hytale-server -- universe --root target/universe list
cargo run -p hytale-server -- serve --universe target/universe --bind 127.0.0.1:5520
cargo run -p hytale-server -- headless --save target/headless-world --chunk 4,-5
cargo run -p hytale-server -- world-boot --save target/headless-world \
  --view-radius 1 --chunks-per-tick 4 --set-block 129,250,-159,4
tools/java-dumper/run-save-check.sh target/headless-world 4 -5 129 250 -159 Debug_Test_Block

# authenticated join (the server prints the generated PKCS#8 key):
cargo run -p hytale-server -- chunk-serve --handshake --auth-mode local \
  --save target/auth-join-smoke --bind 127.0.0.1:5531 --view-radius 0
cargo run -p client-bot -- --host 127.0.0.1:5531 --auth-join \
  --auth-key "<pkcs8 base64 printed by the server>" --username AuthBot \
  --view-radius 0 --asset-limit 2 --move-chunk 1,0 --timeout 60
```

## Working rules

- One task per branch/worktree; shared files (`Cargo.toml`, `plan.md`,
  `NEXT_STEPS.md`, `tasks/README.md`) are main-agent-owned.
- Golden-first: if a task needs expected bytes and no fixture exists, extend the
  Java dumper instead of inventing values.
- Generated code is write-only.
- No broad refactors; finish the task’s acceptance section and report
  follow-ups.
- Update [`PORTING_HISTORY.md`](PORTING_HISTORY.md) at the end of each session.
