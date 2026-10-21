//! QUIC endpoint construction and client connect helpers.
//!
//! This module is the reusable transport surface that later tasks consume. It
//! deliberately knows nothing about Hytale packets, authentication, or session
//! state: it only establishes mutually authenticated QUIC connections with ALPN
//! [`ALPN_HYTALE_V3`] and captures the peer certificate chain.

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

use quinn::crypto::rustls::{NoInitialCipherSuite, QuicClientConfig, QuicServerConfig};
use rustls::client::danger::ServerCertVerifier;
use rustls::server::danger::ClientCertVerifier;

use crate::identity::PeerIdentity;
use crate::tls::{crypto_provider, TlsIdentity, ALPN_HYTALE_V3};
use crate::verify::{accept_any_client_certificate, accept_any_server_certificate};

/// Everything that can go wrong while building or running the transport.
#[derive(Debug)]
pub enum TransportError {
    /// Binding a UDP socket failed.
    Io(std::io::Error),
    /// Generating a self-signed certificate failed.
    Certificate(rcgen::Error),
    /// A `rustls` configuration option was rejected.
    Tls(rustls::Error),
    /// The QUIC/TLS configuration lacks a usable initial cipher suite.
    QuicConfig(NoInitialCipherSuite),
    /// The remote address could not be resolved or dialed.
    Connect(quinn::ConnectError),
    /// The QUIC handshake or the connection itself failed.
    Connection(quinn::ConnectionError),
    /// Opening or writing to a stream failed.
    Stream(quinn::ClosedStream),
    /// Writing to a stream failed.
    Write(quinn::WriteError),
    /// Reading from a stream failed.
    Read(quinn::ReadError),
    /// Reading a stream to completion failed.
    ReadToEnd(quinn::ReadToEndError),
    /// The server endpoint stopped accepting connections.
    EndpointClosed,
    /// The peer completed the handshake without presenting a certificate.
    MissingPeerIdentity,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Certificate(error) => write!(f, "certificate generation failed: {error}"),
            Self::Tls(error) => write!(f, "TLS configuration failed: {error}"),
            Self::QuicConfig(error) => write!(f, "QUIC configuration failed: {error}"),
            Self::Connect(error) => write!(f, "could not connect: {error}"),
            Self::Connection(error) => write!(f, "connection failed: {error}"),
            Self::Stream(error) => write!(f, "stream is closed: {error}"),
            Self::Write(error) => write!(f, "stream write failed: {error}"),
            Self::Read(error) => write!(f, "stream read failed: {error}"),
            Self::ReadToEnd(error) => write!(f, "reading stream to end failed: {error}"),
            Self::EndpointClosed => write!(f, "server endpoint closed"),
            Self::MissingPeerIdentity => {
                write!(f, "peer did not present a certificate during the handshake")
            }
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Certificate(error) => Some(error),
            Self::Tls(error) => Some(error),
            Self::QuicConfig(error) => Some(error),
            Self::Connect(error) => Some(error),
            Self::Connection(error) => Some(error),
            Self::Stream(error) => Some(error),
            Self::Write(error) => Some(error),
            Self::Read(error) => Some(error),
            Self::ReadToEnd(error) => Some(error),
            Self::EndpointClosed | Self::MissingPeerIdentity => None,
        }
    }
}

impl From<std::io::Error> for TransportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rcgen::Error> for TransportError {
    fn from(error: rcgen::Error) -> Self {
        Self::Certificate(error)
    }
}

impl From<rustls::Error> for TransportError {
    fn from(error: rustls::Error) -> Self {
        Self::Tls(error)
    }
}

impl From<NoInitialCipherSuite> for TransportError {
    fn from(error: NoInitialCipherSuite) -> Self {
        Self::QuicConfig(error)
    }
}

impl From<quinn::ConnectError> for TransportError {
    fn from(error: quinn::ConnectError) -> Self {
        Self::Connect(error)
    }
}

impl From<quinn::ConnectionError> for TransportError {
    fn from(error: quinn::ConnectionError) -> Self {
        Self::Connection(error)
    }
}

impl From<quinn::ClosedStream> for TransportError {
    fn from(error: quinn::ClosedStream) -> Self {
        Self::Stream(error)
    }
}

impl From<quinn::WriteError> for TransportError {
    fn from(error: quinn::WriteError) -> Self {
        Self::Write(error)
    }
}

impl From<quinn::ReadError> for TransportError {
    fn from(error: quinn::ReadError) -> Self {
        Self::Read(error)
    }
}

impl From<quinn::ReadToEndError> for TransportError {
    fn from(error: quinn::ReadToEndError) -> Self {
        Self::ReadToEnd(error)
    }
}

/// Builder for a mutually authenticated QUIC server endpoint.
///
/// By default the endpoint requires a client certificate and accepts any
/// self-signed certificate via [`AcceptAnyClientCertificate`]. The certificate
/// policy is injected as a rustls [`ClientCertVerifier`], so the real
/// `cnf.x5t#S256` identity check can be layered in without changing this type:
///
/// [`AcceptAnyClientCertificate`]: crate::AcceptAnyClientCertificate
///
/// ```no_run
/// use std::net::SocketAddr;
/// use std::sync::Arc;
///
/// use hytale_net::verify::AcceptAnyClientCertificate;
/// use hytale_net::{ServerEndpointBuilder, TlsIdentity};
///
/// let identity = TlsIdentity::generate("localhost").unwrap();
/// let server = ServerEndpointBuilder::new(
///     SocketAddr::from(([127, 0, 0, 1], 0)),
///     identity,
/// )
/// .with_client_cert_verifier(Arc::new(AcceptAnyClientCertificate))
/// .bind()
/// .unwrap();
/// # let _ = server;
/// ```
pub struct ServerEndpointBuilder {
    bind_addr: SocketAddr,
    identity: TlsIdentity,
    client_cert_verifier: Arc<dyn ClientCertVerifier>,
    alpn_protocols: Vec<Vec<u8>>,
}

impl ServerEndpointBuilder {
    /// Creates a builder binding to `bind_addr` and presenting `identity`.
    pub fn new(bind_addr: SocketAddr, identity: TlsIdentity) -> Self {
        Self {
            bind_addr,
            identity,
            client_cert_verifier: accept_any_client_certificate(),
            alpn_protocols: vec![ALPN_HYTALE_V3.to_vec()],
        }
    }

    /// Replaces the client certificate policy.
    ///
    /// The verifier must require client authentication (`client_auth_mandatory`)
    /// for [`ServerEndpoint::accept`] to return a [`PeerIdentity`].
    pub fn with_client_cert_verifier(mut self, verifier: Arc<dyn ClientCertVerifier>) -> Self {
        self.client_cert_verifier = verifier;
        self
    }

    /// Overrides the advertised ALPN protocols (default: `hytale/3`).
    pub fn with_alpn_protocols(mut self, protocols: impl IntoIterator<Item = Vec<u8>>) -> Self {
        self.alpn_protocols = protocols.into_iter().collect();
        self
    }

    /// Builds the TLS configuration and binds the UDP socket.
    pub fn bind(self) -> Result<ServerEndpoint, TransportError> {
        let mut tls = rustls::ServerConfig::builder_with_provider(crypto_provider())
            .with_safe_default_protocol_versions()?
            .with_client_cert_verifier(self.client_cert_verifier)
            .with_single_cert(
                self.identity.cert_chain().to_vec(),
                self.identity.private_key().clone_key(),
            )?;
        tls.alpn_protocols = self.alpn_protocols;

        let quic = QuicServerConfig::try_from(tls)?;
        let server_config = quinn::ServerConfig::with_crypto(Arc::new(quic));
        let endpoint = quinn::Endpoint::server(server_config, self.bind_addr)?;
        let local_addr = endpoint.local_addr()?;

        Ok(ServerEndpoint {
            endpoint,
            local_addr,
        })
    }
}

/// A bound QUIC server endpoint.
pub struct ServerEndpoint {
    endpoint: quinn::Endpoint,
    local_addr: SocketAddr,
}

impl ServerEndpoint {
    /// The address the endpoint is listening on.
    ///
    /// For a builder bound to port 0 this is the OS-assigned port.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The underlying `quinn` endpoint, for advanced configuration.
    pub fn endpoint(&self) -> &quinn::Endpoint {
        &self.endpoint
    }

    /// Waits for the next connection, completing the QUIC/TLS handshake.
    ///
    /// Returns [`TransportError::MissingPeerIdentity`] if the peer did not
    /// present a certificate, which should not happen with the default
    /// `ClientAuth.REQUIRE` policy but is possible with a custom verifier that
    /// makes client authentication optional.
    pub async fn accept(&self) -> Result<AcceptedConnection, TransportError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or(TransportError::EndpointClosed)?;
        let connection = incoming.await?;
        let peer_identity = PeerIdentity::from_connection(&connection)
            .ok_or(TransportError::MissingPeerIdentity)?;

        Ok(AcceptedConnection {
            connection,
            peer_identity,
        })
    }

    /// Waits for the next incoming connection without completing the handshake.
    pub async fn accept_incoming(&self) -> Option<quinn::Incoming> {
        self.endpoint.accept().await
    }

    /// Closes the endpoint, refusing new connections.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"server shutdown");
    }
}

/// A server-side connection with its peer's certificate chain.
pub struct AcceptedConnection {
    connection: quinn::Connection,
    peer_identity: PeerIdentity,
}

impl AcceptedConnection {
    /// The established QUIC connection.
    pub fn connection(&self) -> &quinn::Connection {
        &self.connection
    }

    /// The certificate chain the client presented.
    pub fn peer_identity(&self) -> &PeerIdentity {
        &self.peer_identity
    }

    /// Consumes this value, returning the raw connection and peer identity.
    pub fn into_parts(self) -> (quinn::Connection, PeerIdentity) {
        (self.connection, self.peer_identity)
    }
}

/// A bound client endpoint that can dial servers repeatedly.
pub struct ClientEndpoint {
    endpoint: quinn::Endpoint,
}

impl ClientEndpoint {
    /// Binds a client endpoint presenting `identity` as its client certificate.
    ///
    /// The default server certificate policy accepts any certificate
    /// (spike-only, see [`crate::AcceptAnyServerCertificate`]). Use
    /// [`ClientEndpoint::bind_with_verifier`] to supply a real verifier.
    pub fn bind(bind_addr: SocketAddr, identity: &TlsIdentity) -> Result<Self, TransportError> {
        Self::bind_with_verifier(bind_addr, identity, accept_any_server_certificate())
    }

    /// Binds a client endpoint with a custom server certificate policy.
    pub fn bind_with_verifier(
        bind_addr: SocketAddr,
        identity: &TlsIdentity,
        server_cert_verifier: Arc<dyn ServerCertVerifier>,
    ) -> Result<Self, TransportError> {
        let mut tls = rustls::ClientConfig::builder_with_provider(crypto_provider())
            .with_safe_default_protocol_versions()?
            .dangerous()
            .with_custom_certificate_verifier(server_cert_verifier)
            .with_client_auth_cert(
                identity.cert_chain().to_vec(),
                identity.private_key().clone_key(),
            )?;
        tls.alpn_protocols = vec![ALPN_HYTALE_V3.to_vec()];

        let quic = QuicClientConfig::try_from(tls)?;
        let mut endpoint = quinn::Endpoint::client(bind_addr)?;
        endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(quic)));

        Ok(Self { endpoint })
    }

    /// The underlying `quinn` endpoint.
    pub fn endpoint(&self) -> &quinn::Endpoint {
        &self.endpoint
    }

    /// The local UDP address of this endpoint.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Connects to `server_addr`, completing the QUIC/TLS handshake.
    ///
    /// `server_name` is the SNI/hostname the server certificate is checked
    /// against. With the default accept-any policy it is only sent as SNI, but
    /// a real verifier will validate it.
    pub async fn connect(
        &self,
        server_addr: SocketAddr,
        server_name: &str,
    ) -> Result<ClientSession, TransportError> {
        let connection = self.endpoint.connect(server_addr, server_name)?.await?;
        let peer_identity = PeerIdentity::from_connection(&connection);

        Ok(ClientSession {
            _endpoint: self.endpoint.clone(),
            connection,
            peer_identity,
        })
    }
}

impl fmt::Debug for ClientEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientEndpoint")
            .field("local_addr", &self.endpoint.local_addr())
            .finish()
    }
}

/// A client endpoint plus its established connection.
///
/// Holds a handle to the endpoint so the connection's driver keeps running for
/// the lifetime of the session.
pub struct ClientSession {
    _endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    peer_identity: Option<PeerIdentity>,
}

impl ClientSession {
    /// The established QUIC connection.
    pub fn connection(&self) -> &quinn::Connection {
        &self.connection
    }

    /// The server certificate chain, if the server presented one.
    pub fn peer_identity(&self) -> Option<&PeerIdentity> {
        self.peer_identity.as_ref()
    }

    /// Consumes the session, returning the endpoint handle and connection.
    ///
    /// The endpoint must stay alive for as long as the connection is used.
    pub fn into_parts(self) -> (quinn::Endpoint, quinn::Connection) {
        (self._endpoint, self.connection)
    }
}

/// Binds a client endpoint and connects it to `server_addr` in one step.
///
/// This is a convenience wrapper over [`ClientEndpoint::bind`] followed by
/// [`ClientEndpoint::connect`]. The returned [`ClientSession`] keeps the
/// endpoint alive.
pub async fn connect(
    bind_addr: SocketAddr,
    server_addr: SocketAddr,
    server_name: &str,
    identity: &TlsIdentity,
) -> Result<ClientSession, TransportError> {
    let endpoint = ClientEndpoint::bind(bind_addr, identity)?;
    endpoint.connect(server_addr, server_name).await
}
