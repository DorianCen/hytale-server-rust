//! `hytale-net` — QUIC transport and connection state machine.
//!
//! Owns the `quinn`/`rustls` listener, ALPN negotiation, mTLS client-certificate
//! capture, per-stream channel mapping, packet framing on the wire, and the
//! `Initial` → `Authentication` → `Setup` → `Game` connection state machine. It
//! turns decoded packets into messages for the owning world thread and never
//! mutates world state directly.

#![deny(clippy::all)]
