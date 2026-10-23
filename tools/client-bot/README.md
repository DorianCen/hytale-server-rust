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
`en-US`). `--token` adds an identity token; auth is otherwise out of scope.

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
21-packet chunk, one `Unload { x, z }`, and the moved-to chunk (21 packets). The
demo server skips auth/setup; the real session state machine will attach the same
receiver after `Setup`/`Game`.
