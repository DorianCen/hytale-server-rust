# Packet trace fixtures

`hytale-net::trace` captures packet metadata at the stream boundary:

```json
{"type":"header","producer":"hytale-server","protocol_crc":-386561135,"protocol_build":258,"protocol_version":3,"format":1}
{"type":"packet","session":1,"seq":32,"stream":"setup","direction":"outbound","packet_id":104,"packet_name":"JoinWorld","channel":"Default","payload_len":26,"payload_sha256":"33af..."}
```

Records contain only the packet id/name, registry channel, QUIC stream,
direction, per-session sequence, wire payload length and SHA-256. Raw payload
bytes are never written, so traces can be committed without redistributing
asset or protocol bodies.

## Live capture (real client runbook)

1. Build the server and create/choose a universe world:

   ```sh
   cargo build -p hytale-server
   cargo run -p hytale-server -- universe --root target/trace-universe create TraceWorld --seed 0
   ```

2. Start the listener with tracing enabled:

   ```sh
   cargo run -p hytale-server -- serve \
     --universe target/trace-universe --world TraceWorld \
     --bind 0.0.0.0:5520 --view-radius 0 \
     --trace target/trace/real-client.jsonl
   ```

   The console prints `packet trace: target/trace/real-client.jsonl` before
   accepting connections.

3. Start the vanilla client and connect to `localhost:5520`. Exercise the v1
   lifecycle: spawn, look around/move, place and break a block, disconnect, then
   reconnect with the same identity.

4. Stop the server cleanly (Ctrl-C). The trace file is flushed on shutdown.

5. Summarize/validate the capture without a client:

   ```sh
   cargo test -p client-bot --test trace_fixture   # validates the committed fixture
   cargo test -p client-bot --test trace_gate -- --ignored  # live bot lifecycle
   ```

   For a new real-client baseline, copy the captured file into
   `testdata/golden/server/trace/` after normalizing it (outbound records, one
   representative per packet type/session, hashes blank for time/entity/list
   payloads) and update the gate.

> Auth note: the default online client uses the live Hytale session authority.
> The current server's local EdDSA profile is for offline/dev verification; use
> the dev/insecure client profile for a local handshake until the live session
> adapter lands. This is an auth-authority limitation, not a packet-order one.

## Real-client observations encoded in the gate

A captured client log (2026-09-08, local `localhost:5520` run, not redistributed)
shows the setup order and one hard channel rule:

- `AuthGrant` → `ServerAuthToken` → `RequiredAssets` → config asset types
  (`BlockTypes`, `BlockHitboxes`, `BlockSoundSets`, `Items`, `Translations`, …)
  → `Stage SettingUp -> Playing` → `Received JoinWorldPacket`.
- `Packet ViewRadius (ID 32) is not valid on channel Default (expected Chunks)`
  when a server wrote it on the control stream during loading. `ViewRadius`
  **must** go on the `Chunks` stream after `ClientReady`.
- `JoinWorld` is handled after the config burst; the trace fixture asserts that
  ordering and the `Chunks` channel for `ViewRadius`, chunk batches and
  `ServerSetBlock`.

The committed `v1-lifecycle.jsonl` is the normalized lifecycle baseline captured
from the bot-driven listener gate (join/move/edit/disconnect/reconnect). It is
not raw client traffic; it exists so the order/channel/hash contract is
repeatable in this repository. Replace it with the real-client capture when one
is available.

## Regenerating the committed fixture

```sh
HYTALE_UPDATE_TRACE=1 cargo test -p client-bot --test trace_gate -- --ignored
```

Review the diff before committing. Dynamic records intentionally keep an empty
`payload_sha256`; don't fill them by hand.
