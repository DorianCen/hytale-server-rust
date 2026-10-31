# T62 — Security, fuzzing and rate limits

**Status:** done (session 67)
**Package:** `hytale-net`, `hytale-protocol`, `hytale-assets`, `hytale-server`, `tools/client-bot`, docs
**Depends on:** T48
**Agent fit:** medium

## Goal

Make the server fail closed under malformed input and abusive clients without
changing legitimate client behaviour. This closes the T62 security slice:
protocol/asset fuzzing, malformed-frame handling, auth-failure coverage, inbound
rate limits, resource ceilings and a dependency audit.

## Delivered

### Session limits and rate limiting

`hytale-server::SessionLimits` (defaults in parentheses) is part of every
`SessionOptions`:

- `max_inbound_packet_bytes` (4 MiB) — applied by `FrameReader` before it
  allocates the payload and re-applied to the declared decompressed size.
- `inbound_packet_burst` (1024) and `inbound_packets_per_second` (512) — a
  monotonic token bucket in `hytale-server::security::TokenBucket` checked in
  the setup, join-wait and live game loops. Exceeding it sends
  `ServerDisconnect("packet rate limit exceeded")` and ends the session.
- `max_view_radius` (16 chunks) — setup and live `ViewRadius` requests are
  clamped; the game loop counts clamps in `GameLoopReport::view_radius_clamps`.
- `max_queued_messages` (4096) — each session's player/entity/block queue is a
  bounded `sync_channel`. A full queue sets `PlayerJoin::overflow` instead of
  blocking the world thread; the session disconnects with
  `ServerDisconnect("client is too slow")` and reports
  `GameLoopReport::queue_overflow`.

`serve` exposes the limits as `--max-view-radius`,
`--max-packets-per-second`, `--packet-burst`, `--max-packet-bytes` and
`--max-queued-messages`. `SessionSummary` carries the rate/queue/clamp counters.

### Duplicate identity

`PlayerRegistry::register` now returns `Result<_, PlayerJoinError>` and refuses
a second live session with the same UUID (`DuplicateIdentity`). The session
sends `ServerDisconnect("player is already connected")`; duplicate identities
can no longer shadow or take over a live player. Registration happens before
the chunk view is added, so a refused join leaks neither a registry entry nor a
loaded view.

### Fuzzing and malformed input

- `crates/hytale-protocol/tests/fuzz_decoder.rs` — deterministic random bytes
  against framing, zstd decompression and the client-to-server decoders used by
  the session loop (movement, interactions, inventory, windows, setup,
  builder-tool, machinima, voice and asset-editor packets).
- `crates/hytale-assets/tests/fuzz_asset_json.rs` — random `Parent` graphs and
  JSON merge shapes through `ParentResolver`/`merge_values`.
- Both default to 10,000 iterations in the fast test tier and read
  `HYTALE_FUZZ_ITERATIONS` for the longer `tools/security.sh` budget.
- `tools/client-bot/tests/security_limits.rs` — loopback abuse coverage:
  - a setup frame flood is disconnected with the rate-limit reason;
  - an inbound frame above `max_inbound_packet_bytes` is rejected as
    `PayloadTooLarge` before the body is allocated.
- Auth-failure paths remain covered by
  `crates/hytale-auth/tests/live_auth.rs` (issuer/scope/expiry/signature,
  audience, certificate `cnf` binding) and the authenticated loopback join.

### Dependency audit

`tools/security.sh` runs both fuzz targets at the requested budget, verifies
`Cargo.lock` with `cargo metadata --locked`, prints `cargo tree --duplicates`,
and runs `cargo audit` / `cargo deny check` when those tools are installed. The
`zip` workspace dependency moved from 2.6.1 to 9.2.0 and `zstd` from 0.13.3 to
0.14.3 (plus `zstd-safe` 8.2.0) to clear RUSTSEC-2026-0367/0368/0369 and
RUSTSEC-2026-0391/0395–0406; `cargo audit` is clean after the upgrade.

## Acceptance

```sh
HYTALE_FUZZ_ITERATIONS=100000 ./tools/security.sh
cargo test -p hytale-server --lib
cargo test -p client-bot --test security_limits -- --ignored
./tools/test.sh fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Fuzz targets complete the requested budget without a panic.
- A frame flood and an oversized frame both fail closed.
- Duplicate identities and slow consumers are refused/disconnected rather than
  queued indefinitely.
- Legitimate join/movement/inventory/window loopback tests stay green.

## Deviations

- Java's movement model is client-authoritative; T62 adds the inbound packet
  budget and the existing finite/world-bounds/512-block step checks, not a
  per-packet speed oracle. Per-packet teleport acknowledgement policy remains a
  follow-up when the client's correction queue is modelled.
- World-content ceilings (NPC spawn rate, prefab/entity count, NPC path-search
  cost) are producer/performance concerns for T63, not client-session abuse.
  Session-level outbound work is already bounded by the queue and view ceilings.
- `cargo-audit`/`cargo-deny` are optional in this environment. When absent the
  script still performs the lockfile and duplicate-dependency audit and prints
  the skipped advisory-database step explicitly.

## Follow-ups

- Add a real cargo-fuzz/libFuzzer target once the toolchain is available; the
  committed deterministic targets remain the CI gate.
- Model Java's teleport-ack queue and add a movement distance budget if
  client-authoritative speed abuse is observed in the wild.
- Extend the dependency policy to `cargo deny` in CI when the advisory tooling
  is part of the build image.
