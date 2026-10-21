//! Connect/ServerDisconnect round-trip against a local mock QUIC server.
//!
//! The test is ignored for the same reason as the transport spike test: it binds
//! loopback UDP sockets, which some sandboxes block.
//!
//! ```sh
//! cargo test -p client-bot -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};

use client_bot::{
    build_connect, build_connect_frame, build_frame, decode_connection_packet, drain_frames,
    DEFAULT_PROTOCOL_BUILD, DEFAULT_PROTOCOL_CRC,
};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::generated::connection::{DisconnectType, ServerDisconnect};

#[tokio::test]
#[ignore = "binds loopback UDP sockets; run explicitly with --ignored"]
async fn connect_and_server_disconnect_round_trip() {
    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let server_identity = TlsIdentity::generate("localhost").expect("server cert");
    let server = ServerEndpointBuilder::new(bind_addr, server_identity)
        .bind()
        .expect("bind");
    let server_addr = server.local_addr();

    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let (mut send, mut recv) = accepted.connection().accept_bi().await.expect("stream");
        let request = recv.read_to_end(4096).await.expect("read connect");
        let mut buffer = request;
        let records = drain_frames(&mut buffer).expect("frames");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].packet_id, 0);
        assert!(decode_connection_packet(0, &records[0].payload)
            .expect("known")
            .is_ok());

        let disconnect = ServerDisconnect {
            reason: None,
            r#type: DisconnectType::Disconnect,
        };
        let frame = build_frame(&disconnect).expect("disconnect frame");
        send.write_all(&frame).await.expect("send disconnect");
        send.finish().expect("finish");
        let _ = accepted.connection().closed().await;
    });

    let client_identity = TlsIdentity::generate("mock-client").expect("client cert");
    let client = connect(bind_addr, server_addr, "localhost", &client_identity)
        .await
        .expect("connect");

    let connect_packet = build_connect(
        DEFAULT_PROTOCOL_CRC,
        DEFAULT_PROTOCOL_BUILD,
        "2026.01.01",
        "en-US",
        None,
    );
    let frame = build_connect_frame(&connect_packet).expect("connect frame");
    let (mut send, mut recv) = client.connection().open_bi().await.expect("stream");
    send.write_all(&frame).await.expect("send");
    send.finish().expect("finish");

    let reply = recv.read_to_end(4096).await.expect("read disconnect");
    let mut buffer = reply;
    let records = drain_frames(&mut buffer).expect("frames");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].packet_id, 2);
    assert!(decode_connection_packet(2, &records[0].payload)
        .expect("known")
        .is_ok());

    client.connection().close(0u32.into(), b"done");
    server_task.await.expect("server task");
}
