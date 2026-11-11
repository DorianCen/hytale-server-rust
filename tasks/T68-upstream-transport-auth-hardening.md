# T68 — Upstream transport and auth hardening (follow-up)

**Status:** done (session 95; TLS deny-list, channel validation and QUIC ceiling)
**Package:** `hytale-net`, `hytale-auth`, `hytale-server` (tests), docs
**Depends on:** T62, T65, T66
**Agent fit:** medium

## Goal

Mirror the Java hardening added between the archived reference (`387dc7e0a`,
2026-10-14) and the current shared source (`b9eb29b30`, 2026-11-05) that changes
security or failure behavior. The wire protocol and asset schemas did not
change; see [`../JAVA_VERSION_DIFF.md`](../JAVA_VERSION_DIFF.md).

Three deltas matter to Rust:

1. **Outbound TLS candidate trust.** Java now rejects chains containing any of
   eight compromised eMudhra intermediate CA SPKIs for its HTTPS clients. The
   Rust auth client has no equivalent deny-list.
2. **Packet channel validation.** Java rejects a packet whose registry channel
   does not match the receiving stream, before buffering the payload. Rust
   checks stage legality in the session and rejects unknown IDs/oversized
   frames, but never checks the channel against the stream.
3. **QUIC payload ceiling.** Java sends at most 1452 bytes per datagram on real
   paths (5000 on loopback, 1200 floor for IPv4/private peers) and paces
   datagrams. Rust uses quinn defaults (`max_udp_payload_size = 1472`) with no
   explicit transport configuration.

Scope is the smallest change that closes each gap. Do not rework the transport
or replace the HTTP stack wholesale.

## Read first

- Java (new tree, `~/Projects/hytale-shared-source`):
  - `HytaleServer/CoreServer/src/main/java/com/hypixel/hytale/security/CertificateTrustPolicy.java`
    and `HytaleServer/CoreServer/src/test/java/com/hypixel/hytale/security/CertificateTrustPolicyTest.java`
    (the 8 denied SPKI SHA-256 values and the trust-manager wrapper).
  - `HytaleServer/CoreServer/src/main/java/com/hypixel/hytale/Main.java`
    (`CertificateTrustPolicy.install()` placement).
  - `HytaleServer/Protocol/src/main/java/com/hypixel/hytale/protocol/io/ConnectionHandler.java`,
    `.../io/netty/PacketDecoder.java`, `.../io/netty/ProtocolUtil.java`.
  - `HytaleServer/CoreServer/src/main/java/com/hypixel/hytale/server/core/io/handlers/GenericPacketHandler.java`,
    `InitialPacketHandler.java`, `SetupPacketHandler.java`,
    `login/HandshakeHandler.java`, `login/PasswordPacketHandler.java`.
  - `HytaleServer/libs/Quiche/src/main/java/com/hypixel/hytale/lib/quiche/QuicheChannel.java`
    (`acceptsPacket`, `checkFrameSize`, `checkHandled`) and
    `QuicheListener.java` (`MAX_SEND_UDP_PAYLOAD_SIZE`,
    `LOOPBACK_MAX_SEND_UDP_PAYLOAD_SIZE`, `FLOOR_MAX_SEND_UDP_PAYLOAD_SIZE`,
    `configFor`) and `QuicheConnection.java` (`TRANSPORT_PROTOCOL_VIOLATION`,
    pacing).
- Rust:
  - `crates/hytale-auth/src/live.rs` (`SessionService::new`, all HTTPS calls).
  - `crates/hytale-net/src/frame_stream.rs` (`next_frame`) and
    `crates/hytale-net/src/chunk_stream.rs` (`ChunkStreamReader::next_frame`).
  - `crates/hytale-net/src/transport.rs` (`server_config`, client config).
  - `crates/hytale-server/src/session.rs` (`SessionError::UnexpectedPacket`,
    state dispatch) and `crates/hytale-protocol/src/generated/registry.rs`
    (`PacketInfo::channel`).

## Deliverables

### 1. Outbound TLS SPKI deny-list

- Add the eight Java `DISTRUSTED_SPKI_SHA256` values as constants (copy them
  from `CertificateTrustPolicy.java`; do not invent values).
- Enforce the check for every HTTPS request made by `hytale-auth` (OAuth device
  flow, game-session, auth-grant/auth-token, JWKS, account data). A chain
  containing a denied public key must fail the request; normal verification
  must stay on.
- `ureq`'s `TlsConfig` exposes root certificates but no custom
  `ServerCertVerifier`. Choose the smallest workable approach and record it in
  the task file under **Deviations**:
  - a rustls `ClientConfig` with a wrapping verifier that delegates to
    `WebPkiServerVerifier` and then rejects denied SPKIs, used by a small
    blocking HTTPS helper; or
  - another client that accepts a preconfigured rustls config.
  Do not add an async runtime just for this and do not disable verification.
- Make the denied set injectable for tests (e.g. a verifier constructed with a
  `HashSet<String>`) so generated test certificates can exercise rejection
  without the real eMudhra keys.
- Tests:
  - SPKI SHA-256 formatting matches the Java uppercase hex form.
  - A generated root/intermediate chain whose intermediate key is in the
    injected deny set is rejected.
  - A generated chain with no denied key is accepted.
  - Existing auth unit tests stay green (including the live JWT/JWKS paths that
    do not perform network I/O).

### 2. Packet channel validation

- Validate `PacketInfo::channel` against the stream before allocating or
  decompressing the payload. The 8-byte frame header already carries the packet
  ID, so this is a check in `FrameStream::next_frame`; add the expected
  `NetworkChannel` (default `Default`) at construction.
- Game/setup traffic (`Default`) must reject `Voice` packets, matching Java's
  `acceptsPacket`. `Chunks` and `WorldMap` streams must only carry packets of
  their own channel; add the same expected-channel check to `ChunkStreamReader`
  (and its writer if it does not already enforce it).
- Keep the existing stage checks in `session.rs`; do not move message handling.
- Tests:
  - A `Voice`-channel packet on the default stream returns a protocol error.
  - A `Default` packet on a chunks stream returns a protocol error (reader
    unit test with a synthetic stream).
  - Normal join/move/edit/chunk traces and the `client-bot` loopbacks stay
    green.

### 3. QUIC payload ceiling

- Build an explicit `quinn::TransportConfig` with
  `max_udp_payload_size(1452)` and apply it to both server and client
  `quinn::*Config` (`server_config`, client config). Leave MTU discovery
  enabled.
- Loopback: Java uses a 5000-byte ceiling, but quinn transport config is
  per-endpoint, not per-peer. Do **not** add a second loopback-only endpoint as
  part of this task; record the throughput difference as a follow-up and note
  it in `BENCHMARKS.md` only if a benchmark shows a material regression.
- Tests: assert the configured value in a unit test; existing loopback
  chunk-stream/E2E and fast-tier tests stay green.

### 4. Docs and close-out

- This task file is the record; add findings/deviations to it as they land.
- Update `PORTING_HISTORY.md` at the end of the session.
- If the chosen TLS mechanism changes `crates/hytale-auth/Cargo.toml`, keep the
  dependency footprint minimal and note it.

## Acceptance

```sh
./tools/test.sh fast
cargo test -p hytale-net -p hytale-auth
cargo clippy -p hytale-net -p hytale-auth -p hytale-server --all-targets -- -D warnings
cargo fmt -p hytale-net -p hytale-auth -p hytale-server -- --check
```

- An outbound chain containing a denied SPKI is rejected and a normal chain is
  accepted (unit tests with generated certificates).
- A packet on the wrong channel for its stream is rejected with a protocol
  error (unit tests), while valid traffic is unchanged.
- Server and client QUIC configs use a 1452-byte maximum UDP payload.
- The live release-client join/block loop is unchanged; no wire-visible
  difference for valid clients.

## Landed (session 95)

- **Outbound TLS:** replaced `ureq` in `hytale-auth` with a small blocking
  rustls HTTP/1.1 client (`crates/hytale-auth/src/https.rs`). Its verifier wraps
  `WebPkiServerVerifier` and rejects the eight Java `DISTRUSTED_SPKI_SHA256`
  values on the end-entity and intermediate certificates. SPKI extraction is a
  bounded DER walk; the denied set is injectable for tests. Plain `http://` is
  accepted only for loopback hosts so the existing local auth-service tests can
  keep using their stub servers; production URLs remain `https://`.
- **Channel validation:** `FrameReader` defaults to the `Default` stream and
  rejects `Voice` (while allowing the other non-voice channels, matching Java's
  `acceptsPacket`); `ChunkStreamReader` and both chunk/world-map writers require
  an exact `Chunks`/`WorldMap` match. Frames are rejected before payload
  allocation/decompression.
- **QUIC ceiling:** server and client endpoints are built with an explicit
  `quinn::EndpointConfig` whose `max_udp_payload_size` is 1452; MTU discovery
  remains enabled (its upper bound is already 1452).
- **Tests:** verifier rejection/acceptance with generated root/intermediate
  chains, SPKI/HTTP parsing tests, a loopback TLS request through the new
  client, channel-policy unit tests, ignored loopback tests for a `Default`
  packet on the chunks stream and `Voice` on the default stream, plus the
  existing chunk-stream/QUIC/client-bot loopbacks.

## Verification (session 95)

```sh
cargo test -p hytale-auth                             # 10 unit + 8 live-auth passed
cargo test -p hytale-net                              # 3 unit passed, 5 ignored
cargo test -p hytale-net -- --ignored                 # 5 passed (channels/chunk/quic)
cargo test -p client-bot --test commands -- --ignored   # 2 passed
./tools/test.sh fast                                    # 1613 passed, 28 skipped
cargo clippy -p hytale-net -p hytale-auth -p hytale-server --all-targets -- -D warnings
cargo fmt -p hytale-net -p hytale-auth -p hytale-server -- --check
```

## Deviations

- `rustls-webpki` 0.103.16 ships its own `DISTRUSTED_SPKIS` set, but it is a
  different eMudhra key set from Java's eight hashes. The explicit verifier is
  therefore still required; it runs before the WebPKI chain check.
- QUIC's 1452-byte limit is an **endpoint** setting
  (`quinn::EndpointConfig::max_udp_payload_size`), not a per-connection
  `TransportConfig` field. Java's 5000-byte loopback ceiling is not replicated
  because quinn's endpoint config is not per-peer; loopback uses 1452 too.
- The HTTP client is HTTP/1.1 with `Connection: close`, content-length and
  chunked bodies, no redirects/cookies/keep-alive. That matches every live auth
  endpoint; a redirect or non-close response would need a follow-up.
- Plain HTTP is allowed only for loopback hosts (test stubs); production
  `HttpsClient` URLs must be HTTPS.

## Follow-ups

- Loopback QUIC payload/throughput parity (Java 5000 vs quinn's per-endpoint
  ceiling) if benchmarks show it matters.
- Bundled JWKS for offline auth (from the same upstream delta); keep it with
  the credential-persistence follow-up.
- Refresh the asset pack and re-baseline content goldens as a separate task.
