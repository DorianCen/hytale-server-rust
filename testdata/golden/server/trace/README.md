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
     --view-radius 1 \
     --trace target/trace/real-client.jsonl
   ```

   The console prints `packet trace: target/trace/real-client.jsonl` before
   accepting connections. The default `--bind 127.0.0.1:5520` also listens on
   `[::1]:5520`, so a client that resolves `localhost` to IPv6 reaches it.

   Keep the first run at `--view-radius` 0-2. The initial snapshot is generated
   before it is streamed, so a cold radius-8 world takes minutes; the client may
   give up while the world thread is still generating. Once the spawn chunks are
   loaded, reconnecting is fast.

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

### T66 real-client smoke checklist

T66 is green against the unmodified 0.7.7 release client (session 93). Keep
this as the regression runbook. On a successful run the trace and client log
should show all of:

- the client reaches `Playing`, receives the full config burst and `JoinWorld`
  without a `WorldLoadFinished` exception;
- the config burst contains `UpdateBlockTypes`, `UpdateItems`,
  `UpdateBlockSets` and `UpdateBlockGroups` (43 groups / packet `78`), plus the
  interaction stores (`UpdateInteractions`, `UpdateRootInteractions`,
  `UpdateUnarmedInteractions`);
- the initial chunk snapshot includes `SetColumn`, `SetChunk`,
  `SetChunkEnvironments` and `SetAbsentChunks` for empty sections;
- the server sends `UpdateVisibleHudComponents`, `CommandTreeSync`, a self
  `EntityUpdates` with model/stats/movement states and `ClientTeleport`;
- inbound `ClientMovement` is answered without disconnect/correction floods;
- a left click sends `SyncInteractionChains` (not `MouseInteraction` on the
  release client) and gets a non-`CancelInteractionChain` ack plus
  `ServerSetBlock`; the broken block drops item entities;
- walking over a drop auto-picks it up (inventory resync + held-item update),
  and F-key `Pickup`/`Pick` also works;
- a right click sends `SyncInteractionChains` for placement; the trace has
  `SyncInteractionChains` acks and a `ServerSetBlock` on the `Chunks` channel,
  and the placed block renders/collides consistently on the client;
- the client log has no `Failed to prepare the item icon atlas` warning and no
  `NullReferenceException`.

Useful debug command while iterating:

```sh
HYTALE_DEBUG_INTERACTIONS=1 ./target/release/hytale-server serve \
  --universe target/universe --world RealTest --auth-mode authenticated \
  --view-radius 2 --trace target/trace/T66c-smoke.jsonl \
  2> target/trace/server-interactions.log
```

The debug log prints each chain's block/normal/fork, operation block/face/
placed id, and the final `Edit/Use/Rejected` outcome. Placement chains put the
target cell in `interactionData[*].blockPosition`; acks must carry the
authoritative `placedBlockId` or the client rolls its prediction back to air.

Send the trace plus the client log lines around `WorldLoadFinished`, `JoinWorld`
and any asset warning.

### If the client reports "QUIC handshake failed"

A 20-second failure with no server-side log line means the client's UDP packets
never reached a listener. The common cause is an address-family mismatch:
`localhost` resolves to `::1` first on many systems, while a server bound to
`127.0.0.1` or `0.0.0.0` only has an IPv4 socket. Check that the startup line
lists the address the client actually dials:

- no `--bind` (recommended): binds `127.0.0.1:5520` **and** `[::1]:5520`;
- `--bind [::]:5520`: dual-stack on all interfaces (IPv6 wildcard);
- `--bind 0.0.0.0:5520`: IPv4 only, so IPv6 `localhost` still fails.

A failed handshake that *does* reach the server now logs
`QUIC handshake failed: QUIC handshake with <peer> failed: ...`, which names
the TLS/ALPN error instead of timing out silently.

> Auth note: a release client requires the live Hytale session authority. Start
> the server with `--auth-mode authenticated`, approve the printed OAuth device
> URL once, and the server will open its game session and accept the client's
> `AuthGrant`/`ServerAuthToken` handshake. `--auth-mode insecure`/`local` only
> works with a development client. You can skip the interactive flow by
> supplying `HYTALE_SERVER_SESSION_TOKEN` and `HYTALE_SERVER_IDENTITY_TOKEN`
> (and optionally `HYTALE_SERVER_AUDIENCE`) from an existing server session.

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
