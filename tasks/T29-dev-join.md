# T29 — Dev/insecure connection handshake and first join

**Status:** done (session 19)
**Package:** `hytale-net`, `hytale-server`, `client-bot`
**Depends on:** T25 (stream transport), T26 (chunk session)
**Agent fit:** medium

## Goal

Run the Java insecure-mode control flow end to end — `Connect` →
`RequestInsecurePlayerOptions` → `InsecurePlayerOptions` → `ConnectAccept` →
`WorldSettings`/`ServerInfo` → `RequestAssets` → `WorldLoadProgress`/
`WorldLoadFinished` → `SetupFinalize` — then stream chunks. This is the first
complete join handshake, using the dev branch the Java server uses when
authentication is disabled.

## Deliverables

- `hytale-net::frame_stream`:
  - generic `FrameReader`/`FrameWriter` for any QUIC stream;
  - `write_packet` (compresses when the packet registry says so), `write_frame`,
    `write_frames`, `finish`;
  - incremental frame reading with registry id/max-size validation and zstd
    decompression;
  - `Frame::decode::<T>()`, `FrameStreamError`.
- `hytale-server::session`:
  - `SessionOptions` (server name, MOTD, max players, default view radius,
    compression, handshake timeout);
  - `serve_connection` performs the handshake on the client-opened
    bidirectional stream and then streams the player view;
  - handles `ViewRadius`, `Ping`→`Pong`, duplicate assets, and `SetupFinalize`;
  - `SessionReport` (UUID, username, view radius, stream counts),
    `SessionError`.
- `hytale-server chunk-serve --handshake` with `--server-name`, `--motd`,
  `--max-players`, `--handshake-timeout`.
- `client-bot dev_join`:
  - `DevJoinOptions`, `DevJoinReport`, `DevJoinError`;
  - performs the insecure flow and then receives/summarizes the chunk stream;
  - CLI `--dev-join`, `--username`, `--view-radius`.
- `tools/client-bot/tests/dev_join.rs` (`#[ignore]`, pack + loopback UDP):
  full handshake + 21 chunk packets.

## Acceptance

```sh
rm -rf target/dev-join-smoke
cargo run -p hytale-server -- chunk-serve --handshake \
    --save target/dev-join-smoke --bind 127.0.0.1:5523 --view-radius 0
# another terminal:
cargo run -p client-bot -- --host 127.0.0.1:5523 --dev-join \
    --view-radius 0 --timeout 60

cargo test -p client-bot --test dev_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The bot reports the handshake packet ids `[364, 14, 20, 223, 21, 22]` and
  `chunk packets: 21`.
- The server reports `handshake ok: RustBot view_radius=0 chunks=1`.

## Deviations

- Development/insecure mode only; there is no OAuth/JWT authentication or
  certificate binding check.
- No common/config asset streaming: `WorldSettings.requiredAssets` is empty and
  `RequestAssets` is answered with progress/finished only.
- No translations, world-load UI details, player add/spawn packets or game
  loop; the chunk stream is opened immediately after `SetupFinalize`.
- `ServerDisconnect` is only sent for a protocol CRC mismatch; other protocol
  violations return an error and close.
- The client certificate policy is still the transport spike's accept-any.
- The world thread player UUID is independent of the UUID sent in
  `InsecurePlayerOptions`; the dev flow only reports it.
- Only one client/connection per demo process.

## Follow-ups

- Port authentication (OAuth/session service, JWT `cnf.x5t#S256` binding).
- Encode the remaining asset packets and stream common/config assets.
- Send player add/spawn/world settings after `SetupFinalize`.
- Add the game packet loop and movement-driven view updates.
- Drive a real vanilla client against the dev flow and capture the trace.
