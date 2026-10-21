//! Certificate verification policies.
//!
//! Both verifiers in this module accept any certificate and exist only for
//! the transport spike. They are the seam that T13 replaces:
//!
//! * [`AcceptAnyClientCertificate`] sits behind
//!   [`rustls::server::danger::ClientCertVerifier`]. The Java server uses the
//!   same effective policy (`ClientAuth.REQUIRE` + `InsecureTrustManagerFactory`):
//!   the handshake requires a certificate, but any self-signed certificate is
//!   accepted. The real identity check happens after the handshake by matching
//!   the JWT `cnf.x5t#S256` claim against [`crate::PeerIdentity`].
//! * [`AcceptAnyServerCertificate`] lets the spike client connect to the
//!   self-signed server. A production client should install a verifier backed
//!   by the Hytale service roots instead.
//!
//! Keeping these behind rustls' verifier traits means the transport builders do
//! not need to change when stricter verification is introduced: pass a different
//! `Arc<dyn ClientCertVerifier>` to
//! [`crate::ServerEndpointBuilder::with_client_cert_verifier`].

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, Error, SignatureScheme};

use crate::tls::crypto_provider;

/// Server-side policy that requires a client certificate but accepts any
/// self-signed certificate.
///
/// **Spike only.** This authenticates nothing; it only proves that a
/// certificate was presented and makes the chain available via
/// [`crate::PeerIdentity`].
#[derive(Debug, Default, Clone, Copy)]
pub struct AcceptAnyClientCertificate;

impl ClientCertVerifier for AcceptAnyClientCertificate {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &crypto_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &crypto_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        crypto_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Client-side policy that accepts any server certificate.
///
/// **Spike only.** It skips hostname, chain, and expiry validation entirely.
#[derive(Debug, Default, Clone, Copy)]
pub struct AcceptAnyServerCertificate;

impl ServerCertVerifier for AcceptAnyServerCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &crypto_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &crypto_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        crypto_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Convenience constructor for the default spike client verifier.
pub fn accept_any_server_certificate() -> Arc<dyn ServerCertVerifier> {
    Arc::new(AcceptAnyServerCertificate)
}

/// Convenience constructor for the default spike server verifier.
pub fn accept_any_client_certificate() -> Arc<dyn ClientCertVerifier> {
    Arc::new(AcceptAnyClientCertificate)
}
