//! Peer identity captured from a completed QUIC handshake.

use rustls::pki_types::CertificateDer;
use sha2::{Digest, Sha256};

/// SHA-256 digest of arbitrary DER bytes.
pub(crate) fn sha256(der: &[u8]) -> [u8; 32] {
    Sha256::digest(der).into()
}

/// Lowercase hex rendering of a 32-byte digest.
pub(crate) fn hex_digest(digest: &[u8; 32]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(64);
    for byte in digest {
        // Writing into a `String` is infallible.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The certificate chain presented by the peer during the TLS handshake.
///
/// On the server this is the client's chain (from the mTLS handshake); on the
/// client it is the server's chain. The leaf is first, matching the order sent
/// on the wire. The leaf fingerprint is the value that a future JWT
/// `cnf.x5t#S256` verifier compares against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerIdentity {
    chain: Vec<CertificateDer<'static>>,
}

impl PeerIdentity {
    /// Builds an identity from a non-empty certificate chain, leaf first.
    ///
    /// Returns `None` if `chain` is empty.
    pub fn new(chain: Vec<CertificateDer<'static>>) -> Option<Self> {
        if chain.is_empty() {
            None
        } else {
            Some(Self { chain })
        }
    }

    /// Extracts the peer certificate chain from a `quinn` connection.
    ///
    /// `quinn` stores the `rustls` peer certificates as
    /// `Vec<CertificateDer<'static>>` inside the connection's handshake data.
    /// Returns `None` if the handshake has not completed or the peer did not
    /// present a certificate.
    pub fn from_connection(connection: &quinn::Connection) -> Option<Self> {
        let identity = connection.peer_identity()?;
        let chain = identity.downcast::<Vec<CertificateDer<'static>>>().ok()?;
        Self::new(*chain)
    }

    /// The full chain, leaf first.
    pub fn certificates(&self) -> &[CertificateDer<'static>] {
        &self.chain
    }

    /// The leaf (end-entity) certificate.
    pub fn leaf(&self) -> &CertificateDer<'static> {
        // `PeerIdentity` is only constructed through `new`, which rejects
        // empty chains.
        &self.chain[0]
    }

    /// SHA-256 digest of the DER-encoded leaf certificate.
    pub fn leaf_sha256(&self) -> [u8; 32] {
        sha256(self.leaf())
    }

    /// Lowercase hex SHA-256 fingerprint of the leaf certificate.
    pub fn fingerprint_hex(&self) -> String {
        hex_digest(&self.leaf_sha256())
    }

    /// Base64url (no padding) SHA-256 fingerprint, as used by the JWT
    /// `cnf.x5t#S256` certificate binding (RFC 8705).
    pub fn fingerprint_base64url(&self) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine as _;
        URL_SAFE_NO_PAD.encode(self.leaf_sha256())
    }

    /// Returns whether the leaf certificate's SHA-256 digest equals `expected`.
    ///
    /// This is the hook for the future `cnf.x5t#S256` check. The comparison is
    /// an ordinary equality check: certificate fingerprints are public values,
    /// so timing does not need to be constant. `expected` is the raw 32-byte
    /// digest (decode the JWT claim from base64url before calling this).
    pub fn matches_leaf_sha256(&self, expected: &[u8; 32]) -> bool {
        self.leaf_sha256() == *expected
    }
}
