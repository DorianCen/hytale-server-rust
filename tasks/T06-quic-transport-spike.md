# T06 — QUIC transport spike

**Status:** todo
**Agent fit:** low/medium (TLS/QUIC behavior; use a careful agent)
**Depends on:** T01
**Owns:** `crates/hytale-net/**`

## Goal

Prove the transport choice end to end: a `quinn` server accepts a `quinn`
client over QUIC with mTLS, ALPN `hytale/3`, and exposes the client certificate.
No Hytale packets yet — just a framed `ping`/`pong` payload.

## Read

- `CoreServer/.../io/transport/QUICTransport.java`
- `CoreServer/.../io/transport/Transport.java`
- `CoreServer/.../io/netty/HytaleChannelInitializer.java` (stream setup only)
- `Protocol/.../io/netty/PacketEncoder.java` (framing, for the next task)
- [`ARCHITECTURE.md`](../ARCHITECTURE.md) §8

## Work

1. Add `quinn`, `rustls`, `rcgen`, `tokio` to `hytale-net` (workspace deps from
   T01; adjust versions if needed).
2. Implement a spike in `crates/hytale-net/examples/quic_spike.rs`:
   - generate a self-signed server certificate with `rcgen`;
   - server endpoint with ALPN `hytale/3`;
   - require client certificates but accept self-signed ones (this mirrors the
     Java `ClientAuth.REQUIRE` + `InsecureTrustManagerFactory` setup);
   - client endpoint with its own self-signed cert;
   - client opens a bidirectional stream, sends `b"ping"`, server replies
     `b"pong"`.
3. Expose the peer certificate from the server connection (downcast to the
   certificate list) and print its SHA-256 fingerprint in the example. This is
   the input to JWT certificate binding later.
4. Keep the certificate verifier isolated behind a trait/module so the real
   `cnf.x5t#S256` check can replace “accept any” without rewriting the transport.
5. Add a `#[ignore]` integration test that starts the server and client on
   localhost and asserts the round-trip. Document why it is ignored (flaky in
   sandboxes) and how to run it.
6. Expose a small reusable API in `lib.rs` (not only the example): a server
   endpoint builder, a client connect function, and a `PeerIdentity` containing
   the peer certificate chain. T13 will consume this.
7. Do not add Hytale packet decoding; T04/T05 own that.

## Out of scope

- Auth/OAuth/JWT.
- Stream/channel routing and packet state machine.
- Session management.

## Acceptance

```sh
cargo run -p hytale-net --example quic_spike
cargo test -p hytale-net -- --ignored
cargo clippy -p hytale-net --all-targets -- -D warnings
cargo fmt -p hytale-net -- --check
```

The example prints the client certificate fingerprint and `pong`.

## Handoff

Report the quinn/rustls versions, the verifier seam, and how to run the ignored
test.
