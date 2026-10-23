# client-bot

Minimal scripted Hytale client. It generates a self-signed client certificate,
connects over QUIC with ALPN `hytale/3`, sends one framed `Connect`, and logs
the framed packets the server sends until it disconnects or the timeout fires.

```sh
cargo run -p client-bot -- --host 127.0.0.1:5520
cargo run -p client-bot -- --dry-run
```

Defaults match the checked-in protocol constants
(`PROTOCOL_CRC = -386561135`, build `258`, client type `Game`, language
`en-US`). `--token` adds a raw identity token; use `--auth-join` for the full
authenticated flow.

`--dry-run` prints the Connect payload and frame bytes without creating a
socket. The mock-server test is `#[ignore]`d because it binds loopback UDP:

```sh
cargo test -p client-bot -- --ignored
```

Exit code 0 means a clean disconnect or timeout, 1 means a protocol/transport
error.

## Chunk stream receiver

`client_bot::receive_chunk_stream(&ClientSession)` accepts the server's
`Chunks` uni stream, reads it to FIN, and returns `ChunkPacketSummary` values for
`SetColumn`, `SetChunkTintmap`, `SetChunk`, `SetChunkEnvironments` and
`SetFluids` (decoded with the generated protocol types). The loopback test:

```sh
cargo test -p client-bot --test chunk_stream -- --ignored
```

`--chunks` attaches the receiver to a live server. With the transport demo:

```sh
# terminal 1
cargo run -p hytale-server -- chunk-serve \
    --save target/chunk-serve-smoke --bind 127.0.0.1:5521 --view-radius 0
# terminal 2
cargo run -p client-bot -- --host 127.0.0.1:5521 --chunks --timeout 60
```

It prints one `ChunkPacketSummary` per packet and exits 0 when the stream
finishes. With `chunk-serve --move-demo` it prints 43 summaries: the initial
21-packet chunk, one `Unload { x, z }`, and the moved-to chunk (21 packets).

## Dev join

`--dev-join` performs the insecure-mode join and then receives the chunk stream:

```sh
# terminal 1
cargo run -p hytale-server -- chunk-serve --handshake \
    --save target/dev-join-smoke --bind 127.0.0.1:5523 --view-radius 0
# terminal 2
cargo run -p client-bot -- --host 127.0.0.1:5523 --dev-join \
    --view-radius 0 --timeout 60
```

The bot sends `Connect`, answers `RequestInsecurePlayerOptions` with
`InsecurePlayerOptions`, requests no common assets, and sends `SetupFinalize`
after `WorldLoadFinished`. It then `ClearChunks`/`JoinWorld` handshakes with
`ClientReady`, receives the player list entry and self entity spawn, prints the
handshake packet ids, the Game state and the 21 chunk summaries.

## Authenticated join

`--auth-join` runs the local EdDSA flow: it signs a certificate-bound identity
token, answers `AuthGrant` with a matching access token, accepts
`ServerAuthToken`, requests the advertised common assets and verifies every
blob's SHA-256. Use the PKCS#8 key printed by the server:

```sh
# terminal 1 — prints a generated key when --auth-key is omitted
cargo run -p hytale-server -- chunk-serve --handshake --auth-mode local \
    --save target/auth-join-smoke --bind 127.0.0.1:5531 --view-radius 0
# terminal 2
cargo run -p client-bot -- --host 127.0.0.1:5531 --auth-join \
    --auth-key "<pkcs8 base64 from the server>" --username AuthBot \
    --view-radius 0 --asset-limit 2 --move-chunk 1,0 --timeout 60
```

`--asset-limit N` requests only the first `N` advertised assets (omit it to
request all 28k+/45 MB). `--auth-issuer`/`--auth-audience` must match the
server. After the initial snapshot the bot sends
`ClientReady(readyForGameplay)`, receives `ClientTeleport`, and (with
`--move-chunk x,z`) sends a `ClientMovement` teleport ack + position so the
server streams the unload/new-chunk batch. It prints the token exchange,
per-asset hashes/bytes, config packet ids, the Game state, the initial chunk
summaries and the view-update packets. The loopback test is `#[ignore]`d:

```sh
cargo test -p client-bot --test auth_join -- --ignored
```
