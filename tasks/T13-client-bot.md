# T13 — Rust client bot skeleton

**Status:** done (session 2)
**Agent fit:** medium (needs T04/T06; small CLI)
**Depends on:** T04, T06
**Owns:** `tools/client-bot/**`

## Goal

A Rust CLI that opens a QUIC connection to a Hytale server, sends the initial
`Connect` packet, and logs framed packets. This is the first scripted client for
handshake and join tests; it does not need to render or play.

## Read

- `crates/hytale-protocol/src/generated/connection.rs` (T04)
- `crates/hytale-protocol/src/frame.rs` (T05)
- `crates/hytale-net` spike (T06)
- `CoreServer/.../io/handlers/InitialPacketHandler.java` (Connect handling only)
- `Protocol/.../packets/connection/Connect.java`

## Work

1. Create `tools/client-bot` with `clap`:
   ```text
   client-bot --host 127.0.0.1:5520 --protocol-crc <crc> --build 258
   ```
2. Generate a self-signed client cert; connect with ALPN `hytale/3`; require the
   server cert but accept self-signed (test mode only).
3. Send `Connect` with:
   - protocol CRC from `ProtocolSettings`;
   - build number;
   - client version string;
   - client type `Game`;
   - language `en-US`;
   - no identity token in the skeleton (add `--token` later).
4. Read frames, decode known packets (`ServerDisconnect`, `ConnectAccept`,
   `AuthGrant`, `Ping`/`Pong`), and print `id len name` for others.
5. Exit codes: 0 on clean disconnect, 1 on protocol/transport error.
6. Tests:
   - frame/Connect round-trip through a mock server;
   - a `--dry-run` mode that prints the Connect bytes without connecting.

## Out of scope

- No auth flow, no asset download, no world join.
- No bot movement/AI.
- No changes to `hytale-net` beyond the public transport API.

## Acceptance

```sh
cargo run -p client-bot -- --dry-run
cargo test -p client-bot
cargo clippy -p client-bot --all-targets -- -D warnings
cargo fmt -p client-bot -- --check
```

If a Java server is available, `client-bot` reaches the auth/setup stage and
logs the server’s reply.

## Handoff

Report the CLI, the observed handshake stage, and what the next packet to
implement is.
