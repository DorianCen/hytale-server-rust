//! `hytale-net` — QUIC transport and connection state machine.
//!
//! Owns the `quinn`/`rustls` listener, ALPN negotiation, mTLS client-certificate
//! capture, per-stream channel mapping, packet framing on the wire, and the
//! `Initial` → `Authentication` → `Setup` → `Game` connection state machine. It
//! turns decoded packets into messages for the owning world thread and never
//! mutates world state directly.
//!
//! # Current scope
//!
//! The crate implements the QUIC endpoint pair with mutual TLS, ALPN
//! [`ALPN_HYTALE_V3`], peer certificate capture through [`PeerIdentity`], the
//! generic framed setup/chunk streams, and the base64url certificate
//! fingerprints used by the JWT `cnf.x5t#S256` binding. Authentication and the
//! `Initial` → `Setup` → `Game` state machine live in
//! `hytale-server::session`/`hytale-auth`; this crate only moves frames.
//!
//! ```no_run
//! use std::net::SocketAddr;
//!
//! use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
//!
//! # async fn example() -> Result<(), hytale_net::TransportError> {
//! let server_identity = TlsIdentity::generate("localhost")?;
//! let server = ServerEndpointBuilder::new(
//!     SocketAddr::from(([127, 0, 0, 1], 0)),
//!     server_identity,
//! )
//! .bind()?;
//!
//! let client_identity = TlsIdentity::generate("client")?;
//! let client = connect(
//!     SocketAddr::from(([127, 0, 0, 1], 0)),
//!     server.local_addr(),
//!     "localhost",
//!     &client_identity,
//! )
//! .await?;
//!
//! let accepted = server.accept().await?;
//! println!(
//!     "client certificate sha256: {}",
//!     accepted.peer_identity().fingerprint_hex()
//! );
//! # let _ = client;
//! # Ok(())
//! # }
//! ```
//!
//! # Certificate verification
//!
//! The transport accepts any self-signed peer certificate, mirroring the Java
//! server's `ClientAuth.REQUIRE` plus `InsecureTrustManagerFactory` setup. The
//! identity check is the JWT `cnf.x5t#S256` binding performed by
//! `hytale-auth`/`hytale-server::session` against [`PeerIdentity`]; the
//! certificate policy lives entirely in [`verify`] so a CA verifier can replace
//! it without touching [`transport`].

#![deny(clippy::all)]

pub mod chunk_stream;
pub mod frame_stream;
pub mod identity;
pub mod tls;
pub mod transport;
pub mod verify;

pub use chunk_stream::{ChunkFrame, ChunkStreamError, ChunkStreamReader, ChunkStreamWriter};
pub use frame_stream::{
    Frame, FrameReader, FrameStreamError, FrameWriter,
    DEFAULT_COMPRESSION_LEVEL as FRAME_COMPRESSION_LEVEL,
};
pub use identity::PeerIdentity;
pub use tls::{TlsIdentity, ALPN_HYTALE_V3};
pub use transport::{
    connect, AcceptedConnection, ClientEndpoint, ClientSession, ServerEndpoint,
    ServerEndpointBuilder, TransportError,
};
pub use verify::{AcceptAnyClientCertificate, AcceptAnyServerCertificate};
