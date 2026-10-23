# T30 — Authenticated setup, common/config assets, Game transition

**Status:** done (session 20)
**Package:** `hytale-auth`, `hytale-assets`, `hytale-net`, `hytale-server`, `client-bot`
**Depends on:** T29 (dev join), T28 (P0 asset packets), T15 (asset registry)
**Agent fit:** low/medium

## Goal

Move the join flow off the insecure-only branch: authenticate the client with
EdDSA JWTs bound to the mTLS certificate, stream the common-asset blobs the
client requests (`WorldSettings.requiredAssets` → `AssetInitialize`/`AssetPart`/
`AssetFinalize`), send the config asset packets, and drive the control flow into
`Game` (`ClearChunks`, `JoinWorld`, `ClientReady`, the post-join world packets)
before the chunk stream.

This is the offline/private-server authentication profile: it mirrors the Java
JWT claims and the `cnf.x5t#S256` binding but uses a locally configured signing
key instead of the Hytale session service. The packet/state flow matches
`InitialPacketHandler` → `HandshakeHandler` → `PasswordPacketHandler` →
`SetupPacketHandler` → `World.addPlayer`.

## Deliverables

- `hytale-auth`:
  - Ed25519 (`EdDSA`) key generation, PKCS#8 load, sign/verify;
  - JWT encode/decode (base64url, no padding), claims `iss`/`aud`/`sub`/
    `username`/`scope`/`iat`/`nbf`/`exp`/`cnf.x5t#S256` plus a local token-use
    claim;
  - validation: signature, alg, issuer, audience, clock skew, scope, UUID
    subject and certificate binding;
  - `AuthService` issuing identity/access/grant/server tokens for local mode.
- `hytale-net`: base64url certificate fingerprint helpers on `TlsIdentity` and
  `PeerIdentity`.
- `hytale-server::session`:
  - `AuthMode::{Insecure, Local}`;
  - authenticated branch: `Connect(identityToken)` → `AuthGrant` →
    `AuthToken` → `ServerAuthToken` → `WorldSettings`/`ServerInfo`;
  - `WorldSettings.requiredAssets` from the common-asset index;
  - `RequestAssets` subset handling, unknown-hash rejection, per-asset
    `AssetInitialize` + `WorldLoadProgress` + `AssetPart` + `AssetFinalize`,
    then `RequestCommonAssetsRebuild`;
  - config asset packets from the registry (`PhysicalMaterial`,
    `BlockHitboxes` at minimum) and an `UpdateTranslations` init;
  - post-setup `ClearChunks`/`JoinWorld`/`ClientReady` gate, the world setup
    packet burst, `AddToServerPlayerList` and the joining player's self
    `EntityUpdates`, then the existing chunk stream.
- `hytale-assets::common`: `CommonAssetsIndex.hashes` parser, hash/name lookup,
  lazy blob loading with SHA-256 verification.
- `WorldThread`: commands returning the encoded config asset packets and the
  common-asset index (the registry is not loaded twice).
- `client-bot`: `auth_join` flow that signs a certificate-bound identity token,
  answers the grant exchange, requests assets, decodes the asset stream and
  reaches `Game`.
- Loopback tests for the auth token rules and the full join.

## Acceptance

```sh
cargo test -p hytale-auth
cargo test -p hytale-assets
cargo test -p hytale-server
cargo test -p client-bot
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The bot completes the authenticated handshake, receives config asset packets
  and common asset bytes, and reports `Game`.
- An identity token whose `cnf.x5t#S256` does not match the mTLS certificate is
  rejected.
- `WorldSettings.requiredAssets` is non-empty and every requested hash resolves
  to bytes whose SHA-256 matches.

## Deviations (to record in docs)

- Authentication is a local EdDSA profile, not the live Hytale session service
  (no OAuth device/browser flow, JWKS fetch or session registration).
- Config asset streaming is limited to the stores the Rust registry can encode;
  `Environment`/`Fluid`/`BlockType`/`Item` remain follow-ups.
- The post-join burst sends `AddToServerPlayerList`, the self `EntityUpdates`
  spawn (default model/skin/equipment + the saved transform), and the world
  identity packets. Other-player tracking, time/gameplay config and the
  `readyForGameplay` teleport exchange remain T31.
- No password challenge and no referral handling.
- Translations are an empty init packet.
