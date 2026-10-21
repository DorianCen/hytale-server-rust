//! Integration test for the QUIC mutual-TLS transport spike.
//!
//! The test binds UDP sockets on localhost, generates two throwaway
//! self-signed certificates, requires a client certificate on the server (while
//! accepting any certificate), and asserts a `ping`/`pong` round-trip over one
//! bidirectional stream.
//!
//! # Why it is ignored
//!
//! It is `#[ignore]`d because it depends on UDP loopback networking and dynamic
//! port binding. Sandboxed CI environments, locked-down containers, and hosts
//! with restrictive firewalls can block or silently drop loopback UDP, which
//! makes the test flaky or hang. It also generates certificates at runtime,
//! which is unnecessary work for the default test suite. The test body is still
//! compiled with `cargo test`, so it cannot rot.
//!
//! # How to run it
//!
//! ```sh
//! cargo test -p hytale-net -- --ignored
//! # or, with the fingerprint output visible:
//! cargo test -p hytale-net -- --ignored --nocapture
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};

#[tokio::test]
#[ignore = "binds loopback UDP sockets; run explicitly with --ignored"]
async fn quic_mtls_ping_pong() {
    let scenario = async {
        let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));

        let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
        let server_fingerprint = server_identity.fingerprint_hex();
        let server = ServerEndpointBuilder::new(bind_addr, server_identity)
            .bind()
            .expect("bind server endpoint");
        let server_addr = server.local_addr();

        let client_identity = TlsIdentity::generate("hytale-test-client").expect("client cert");
        let client_fingerprint = client_identity.fingerprint_hex();

        let server_task = tokio::spawn(async move {
            let accepted = server.accept().await?;
            let observed_fingerprint = accepted.peer_identity().fingerprint_hex();
            let (mut send, mut recv) = accepted.connection().accept_bi().await?;
            let request = recv.read_to_end(64).await?;
            assert_eq!(request, b"ping");
            send.write_all(b"pong").await?;
            send.finish()?;
            // Stay alive until the client has read the FIN and closed; dropping
            // the connection here can race the stream finish.
            let _ = accepted.connection().closed().await;
            Ok::<_, hytale_net::TransportError>(observed_fingerprint)
        });

        let session = connect(bind_addr, server_addr, "localhost", &client_identity)
            .await
            .expect("connect client");

        let server_peer = session
            .peer_identity()
            .expect("server must present a certificate");
        assert_eq!(server_peer.fingerprint_hex(), server_fingerprint);

        let (mut send, mut recv) = session.connection().open_bi().await.expect("open stream");
        send.write_all(b"ping").await.expect("write ping");
        send.finish().expect("finish ping");
        let response = recv.read_to_end(64).await.expect("read pong");
        assert_eq!(response, b"pong");

        session.connection().close(0u32.into(), b"test complete");
        let observed_client_fingerprint = server_task.await.expect("server task").expect("server");
        assert_eq!(observed_client_fingerprint, client_fingerprint);
    };

    tokio::time::timeout(Duration::from_secs(10), scenario)
        .await
        .expect("QUIC spike timed out");
}
