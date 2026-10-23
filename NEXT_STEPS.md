# Next steps — CoreServer rewrite, phase 2 (session 25)

**Status:** the first vertical slice is archived in
[`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md)
(tasks T01–T33, sessions 1–23). The phase-2 plan is [`plan.md`](plan.md) and
the task board is [`tasks/README.md`](tasks/README.md).

**Next tasks:** T36, T37 and T38 can run in parallel now that the child-store
goldens exist. T36 adds the block/fluid child stores and their packets; T37 does
the item child stores; T38 adds `TagPattern`. T39 wires them into the join.

## Current state

Landed and green:

- Protocol codegen, framing, QUIC/mTLS transport.
- P0 asset registry + deterministic ids; config packets `41`, `87`, `83`, `61`,
  `40`, `54`.
- T35 child-store goldens (26 stores, 21 payloads) with selector/closure support
  in `AssetDumper`; `BlockSet` and `GameplayConfig` gaps are recorded.
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

1. **M2 — asset completion:** T35 → T36/T37/T38 → T39.
2. **M3 — runtime:** T40 → T41 → T42 (parallel with M2 after T34).
3. **M4 — v1:** T43 → T44 → T45 → T46 → T47 → T48.
4. **M5/M6/M7:** gameplay, builtins/NPC, hardening (see [`plan.md`](plan.md)).

## Try it

The phase-1 commands still work:

```sh
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
