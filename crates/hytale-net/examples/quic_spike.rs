//! End-to-end QUIC mTLS transport spike.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p hytale-net --example quic_spike
//! ```
//!
//! Both sides generate throwaway self-signed certificates. The server requires a
//! client certificate but accepts any certificate, mirroring the Java server's
//! `ClientAuth.REQUIRE` + `InsecureTrustManagerFactory` configuration. The
//! example then round-trips a `ping`/`pong` payload over one bidirectional
//! stream and prints each peer's certificate SHA-256 fingerprint.

use std::net::{Ipv4Addr, SocketAddr};

use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity, ALPN_HYTALE_V3};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));

    let server_identity = TlsIdentity::generate("localhost")?;
    let server = ServerEndpointBuilder::new(bind_addr, server_identity).bind()?;
    let server_addr = server.local_addr();
    println!(
        "server listening on {server_addr} (ALPN {})",
        String::from_utf8_lossy(ALPN_HYTALE_V3)
    );

    let client_identity = TlsIdentity::generate("hytale-spike-client")?;
    let expected_client_fingerprint = client_identity.fingerprint_hex();

    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await?;

        let (mut send, mut recv) = accepted.connection().accept_bi().await?;
        let ping = recv.read_to_end(64).await?;
        assert_eq!(ping, b"ping", "unexpected request payload");

        send.write_all(b"pong").await?;
        send.finish()?;

        let client_fingerprint = accepted.peer_identity().fingerprint_hex();
        println!("server: client certificate sha256 = {client_fingerprint}");

        // Keep the connection alive until the client acknowledges the response
        // and closes. Dropping it immediately after `finish()` can race the
        // stream FIN and surface as a connection-lost error on the client.
        let _ = accepted.connection().closed().await;

        Ok::<_, hytale_net::TransportError>(client_fingerprint)
    });

    let session = connect(bind_addr, server_addr, "localhost", &client_identity).await?;
    println!(
        "client: ALPN = {}",
        session
            .connection()
            .handshake_data()
            .and_then(|data| data.downcast::<quinn::crypto::rustls::HandshakeData>().ok())
            .and_then(|data| data.protocol)
            .map(|protocol| String::from_utf8_lossy(&protocol).into_owned())
            .unwrap_or_else(|| "<none>".to_owned())
    );
    if let Some(peer) = session.peer_identity() {
        println!(
            "client: server certificate sha256 = {}",
            peer.fingerprint_hex()
        );
    }

    let (mut send, mut recv) = session.connection().open_bi().await?;
    send.write_all(b"ping").await?;
    send.finish()?;

    let pong = recv.read_to_end(64).await?;
    println!("client received: {}", String::from_utf8_lossy(&pong));
    assert_eq!(pong, b"pong", "unexpected response payload");

    session.connection().close(0u32.into(), b"spike complete");
    let observed_client_fingerprint = server_task.await??;
    assert_eq!(
        observed_client_fingerprint, expected_client_fingerprint,
        "server saw a different client certificate than the one configured"
    );

    Ok(())
}
