# T65 — Live session-service authentication

**Status:** done (session 65; live-client asset follow-up in T66)
**Package:** `hytale-auth`, `hytale-server`, `client-bot`, docs
**Depends on:** T48, T42

## Goal

Let an unmodified release Hytale client complete the authenticated connection
flow. Release clients reject the development `ConnectAccept` path with
`Received ConnectAccept in release build - development flow not allowed`; they
require the Java server's `AuthGrant` → `AuthToken` → `ServerAuthToken`
exchange backed by Hytale's OAuth and session services.

## Deliverables

- `hytale-auth::live`:
  - OAuth device flow (`oauth2/device/auth`, `oauth2/token`);
  - game profile discovery and `/game-session/new` / `/game-session/refresh`;
  - session-service JWKS fetch and Ed25519 (EdDSA) verification;
  - `/server-join/auth-grant` and `/server-join/auth-token` with the server
    certificate fingerprint;
  - identity-token validation (issuer, scope, expiry, subject, profile
    username) and access-token validation (server audience + `cnf.x5t#S256`);
  - external-token mode via `HYTALE_SERVER_SESSION_TOKEN` /
    `HYTALE_SERVER_IDENTITY_TOKEN` and `HYTALE_SERVER_AUDIENCE`.
- `hytale-server`:
  - `--auth-mode authenticated` (device flow at startup, or env tokens);
  - the authenticated `perform_handshake` branch, with the server TLS
    certificate fingerprint registered before accepting connections.
- `client-bot`:
  - `auth_join_with_tokens` so the live token shape can be tested end to end.
- Regression tests:
  - `crates/hytale-auth/tests/live_auth.rs` — claims, signature, JWKS parsing,
    certificate binding, grant/token request shapes, device/game-session
    request shapes;
  - `tools/client-bot/tests/live_auth_join.rs` — full authenticated join with a
    loopback session-service stub, including the `x509Fingerprint` assertion.

## Acceptance

- A release client's `Connect(identityToken)` is validated and answered with
  `AuthGrant`, then `AuthToken` is exchanged for `ServerAuthToken` and setup
  proceeds.
- The access-token audience is the server audience and its `cnf` matches the
  client's mTLS certificate.
- The grant exchange binds the server's own TLS certificate fingerprint.
- `./tools/test.sh fast`, the local `auth_join` E2E and the live
  `live_auth_join` E2E pass.

## Completion note

Verified against the release client: it sent `Connect(identityToken)`, received
`AuthGrant` with the server identity token, validated `ServerAuthToken`, and
entered `SettingUp` (log 2026-10-31 15:37). Subsequent `WorldLoadFinished`
failures are config-asset completeness, not authentication; they are tracked in
[T66](T66-live-client-assets.md).

## Deviations

- The browser PKCE flow, encrypted credential persistence and automatic refresh
  scheduling are not implemented yet. The device flow is interactive once per
  server start; tokens can instead be supplied via the Java-compatible
  environment variables.
- The live identity token's `profile.skin` is mapped to the protocol
  `PlayerSkin` by field name because generated protocol types are not
  `Deserialize`; malformed skin JSON degrades to no cosmetics rather than
  rejecting the join.
- Editor clients remain unsupported.
