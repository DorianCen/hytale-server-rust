//! TLS identities and the QUIC crypto provider.
//!
//! The spike uses self-signed certificates on both ends, because the Java server
//! requires a client certificate but does not validate it against a CA
//! (`InsecureTrustManagerFactory`). Identity is established later by binding a
//! JWT to the certificate fingerprint.

use std::sync::Arc;

use rcgen::CertifiedKey;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

use crate::identity::sha256;
use crate::transport::TransportError;

/// ALPN protocol identifier for Hytale protocol version 3.
pub const ALPN_HYTALE_V3: &[u8] = b"hytale/3";

/// Returns the rustls crypto provider used by this crate.
///
/// `quinn` enables rustls' `ring` backend by default so the transport does not
/// depend on a second crypto provider. The workspace `rustls` dependency also
/// enables `aws-lc-rs`; with both providers compiled in, rustls refuses to pick
/// one implicitly. Selecting `ring` explicitly keeps the configuration
/// deterministic and avoids a process-global provider install.
pub fn crypto_provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// A certificate chain together with its private key.
///
/// For the spike this is a single self-signed certificate. The type is not
/// cloned because [`PrivateKeyDer`] is not cloneable; use
/// [`TlsIdentity::private_key`] and [`PrivateKeyDer::clone_key`] when a TLS
/// configuration needs its own copy.
pub struct TlsIdentity {
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
}

impl TlsIdentity {
    /// Generates a self-signed certificate for `subject_alt_name`.
    ///
    /// `subject_alt_name` may be a DNS name such as `localhost` or an IP
    /// address literal. The generated key is ECDSA P-256 in PKCS#8 form.
    pub fn generate(subject_alt_name: impl Into<String>) -> Result<Self, TransportError> {
        let certified = rcgen::generate_simple_self_signed(vec![subject_alt_name.into()])?;
        Ok(Self::from_certified_key(certified))
    }

    /// Wraps an `rcgen` certificate/key pair.
    pub fn from_certified_key(certified: CertifiedKey) -> Self {
        let key =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()));
        Self {
            chain: vec![certified.cert.der().clone()],
            key,
        }
    }

    /// The certificate chain, leaf first.
    pub fn cert_chain(&self) -> &[CertificateDer<'static>] {
        &self.chain
    }

    /// The private key belonging to the leaf certificate.
    pub fn private_key(&self) -> &PrivateKeyDer<'static> {
        &self.key
    }

    /// SHA-256 digest of the DER-encoded leaf certificate.
    pub fn fingerprint(&self) -> [u8; 32] {
        sha256(self.leaf())
    }

    /// Lowercase hex SHA-256 fingerprint of the leaf certificate.
    pub fn fingerprint_hex(&self) -> String {
        crate::identity::hex_digest(&self.fingerprint())
    }

    /// Base64url (no padding) SHA-256 fingerprint, matching the JWT
    /// `cnf.x5t#S256` claim.
    pub fn fingerprint_base64url(&self) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine as _;
        URL_SAFE_NO_PAD.encode(self.fingerprint())
    }

    /// The DER-encoded leaf certificate.
    pub fn leaf(&self) -> &CertificateDer<'static> {
        &self.chain[0]
    }
}

impl std::fmt::Debug for TlsIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsIdentity")
            .field("chain_len", &self.chain.len())
            .field("leaf_sha256", &self.fingerprint_hex())
            .field("key", &"<redacted>")
            .finish()
    }
}
