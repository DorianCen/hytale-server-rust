//! `hytale-auth` — Hytale session and OAuth integration.
//!
//! Implements the OAuth browser/device flows, session-service exchange, JWT
//! validation and certificate binding (`cnf.x5t#S256`), token refresh, the encrypted
//! credential store, and offline/dev authentication. Every external Hytale
//! service sits behind a trait so a service change does not stall the server.

#![deny(clippy::all)]
