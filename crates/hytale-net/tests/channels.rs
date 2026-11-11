//! Channel validation on the default and chunk streams.
//!
//! Binds loopback UDP, so it is `#[ignore]`d for the default suite:
//!
//! ```sh
//! cargo test -p hytale-net --test channels -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use hytale_net::{
    connect, ChunkStreamError, ChunkStreamReader, ChunkStreamWriter, FrameReader, FrameStreamError,
    FrameWriter, ServerEndpointBuilder, TlsIdentity,
};
use hytale_protocol::frame::encode_frame;
use hytale_protocol::generated::interface_::ServerMessage;
use hytale_protocol::generated::voice::VoiceData;
use hytale_protocol::generated::wire::Packet;

fn wrong_channel_frame(packet_id: u32) -> Vec<u8> {
    let mut frame = Vec::new();
    encode_frame(packet_id, &[], &mut frame).expect("frame");
    frame
}

#[tokio::test]
#[ignore = "binds loopback UDP sockets; run explicitly with --ignored"]
async fn chunk_stream_rejects_a_default_channel_packet() {
    let scenario = async {
        let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
        let server = ServerEndpointBuilder::new(bind_addr, server_identity)
            .bind()
            .expect("bind server");
        let server_addr = server.local_addr();
        let client_identity = TlsIdentity::generate("channel-client").expect("client cert");
        let frame = wrong_channel_frame(ServerMessage::ID);

        let server_task = tokio::spawn(async move {
            let accepted = server.accept().await?;
            let mut writer = ChunkStreamWriter::open(accepted.connection()).await?;
            let rejected = writer
                .write_packet(&ServerMessage::default(), 3)
                .await
                .expect_err("the chunks writer must reject a default-channel packet");
            assert!(matches!(rejected, ChunkStreamError::WrongChannel { .. }));
            writer.write_frame(&frame).await?;
            writer.finish()?;
            let _ = accepted.connection().closed().await;
            Ok::<_, ChunkStreamError>(())
        });

        let session = connect(bind_addr, server_addr, "localhost", &client_identity)
            .await
            .expect("connect");
        let mut reader = ChunkStreamReader::accept(session.connection())
            .await
            .expect("accept chunk stream");
        let error = reader
            .next_frame()
            .await
            .expect_err("a default packet on the chunks stream must be rejected");
        assert!(
            matches!(
                error,
                ChunkStreamError::WrongChannel {
                    packet_id,
                    channel: hytale_protocol::generated::wire::NetworkChannel::Default,
                    expected: hytale_protocol::generated::wire::NetworkChannel::Chunks,
                } if packet_id == ServerMessage::ID
            ),
            "unexpected error: {error}"
        );

        session.connection().close(0u32.into(), b"done");
        server_task.await.expect("server task").expect("server");
    };

    tokio::time::timeout(Duration::from_secs(10), scenario)
        .await
        .expect("channel test timed out");
}

#[tokio::test]
#[ignore = "binds loopback UDP sockets; run explicitly with --ignored"]
async fn default_stream_rejects_a_voice_packet() {
    let scenario = async {
        let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
        let server = ServerEndpointBuilder::new(bind_addr, server_identity)
            .bind()
            .expect("bind server");
        let server_addr = server.local_addr();
        let client_identity = TlsIdentity::generate("voice-client").expect("client cert");
        let frame = wrong_channel_frame(VoiceData::ID);

        let server_task = tokio::spawn(async move {
            let accepted = server.accept().await?;
            let (send, _recv) = accepted.connection().accept_bi().await?;
            let mut writer = FrameWriter::new(send);
            let rejected = writer
                .write_packet(&VoiceData::default(), 3)
                .await
                .expect_err("the default writer must reject voice packets");
            assert!(matches!(rejected, FrameStreamError::WrongChannel { .. }));
            writer.write_frame(&frame).await?;
            writer.finish()?;
            let _ = accepted.connection().closed().await;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        });

        let session = connect(bind_addr, server_addr, "localhost", &client_identity)
            .await
            .expect("connect");
        let (mut send, recv) = session
            .connection()
            .open_bi()
            .await
            .expect("open setup stream");
        // An empty stream is not announced to the peer until it carries data.
        send.write_all(b"x").await.expect("announce the stream");
        let mut reader = FrameReader::new(recv);
        let error = reader
            .next_frame()
            .await
            .expect_err("voice on the default stream must be rejected");
        assert!(
            matches!(
                error,
                FrameStreamError::WrongChannel {
                    packet_id,
                    channel: hytale_protocol::generated::wire::NetworkChannel::Voice,
                    expected: hytale_protocol::generated::wire::NetworkChannel::Default,
                } if packet_id == VoiceData::ID
            ),
            "unexpected error: {error}"
        );

        session.connection().close(0u32.into(), b"done");
        server_task.await.expect("server task").expect("server");
    };

    tokio::time::timeout(Duration::from_secs(10), scenario)
        .await
        .expect("channel test timed out");
}
