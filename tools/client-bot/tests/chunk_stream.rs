//! Loopback test: the bot receives and decodes a `Chunks` stream.
//!
//! `#[ignore]`d like the other QUIC loopback tests; run explicitly:
//!
//! ```sh
//! cargo test -p client-bot --test chunk_stream -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use client_bot::{receive_chunk_stream, ChunkPacketSummary};
use hytale_net::{
    connect, ChunkStreamError, ChunkStreamWriter, ServerEndpointBuilder, TlsIdentity,
};
use hytale_protocol::frame::{compress_payload, encode_frame};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::generated::world::{SetChunk, SetColumn, UnloadChunk};
use hytale_protocol::io::Writer;

fn frame<T: Packet + Wire>(packet: &T) -> Vec<u8> {
    let mut writer = Writer::new();
    packet.encode(&mut writer).expect("encode payload");
    let payload = writer.into_inner();
    let wire = if T::COMPRESSED {
        compress_payload(&payload, 3).expect("compress")
    } else {
        payload
    };
    let mut frame = Vec::new();
    encode_frame(T::ID, &wire, &mut frame).expect("frame");
    frame
}

#[tokio::test]
#[ignore = "binds loopback UDP sockets; run explicitly with --ignored"]
async fn the_bot_decodes_a_chunk_stream() {
    let scenario = async {
        let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
        let server = ServerEndpointBuilder::new(bind_addr, server_identity)
            .bind()
            .expect("bind server");
        let server_addr = server.local_addr();
        let client_identity = TlsIdentity::generate("chunk-bot").expect("client cert");

        let chunk = SetChunk {
            x: 4,
            y: 1,
            z: -5,
            data: Some(vec![9; 16]),
            ..SetChunk::default()
        };
        let column_frame = frame(&SetColumn::default());
        let chunk_frame = frame(&chunk);
        let unload_frame = frame(&UnloadChunk {
            chunk_x: 4,
            chunk_z: -5,
        });

        let server_task = tokio::spawn(async move {
            let accepted = server.accept().await?;
            let mut writer = ChunkStreamWriter::open(accepted.connection()).await?;
            writer
                .write_frames([
                    column_frame.as_slice(),
                    chunk_frame.as_slice(),
                    unload_frame.as_slice(),
                ])
                .await?;
            writer.finish()?;
            let _ = accepted.connection().closed().await;
            Ok::<_, ChunkStreamError>(())
        });

        let session = connect(bind_addr, server_addr, "localhost", &client_identity)
            .await
            .expect("connect");
        let packets = receive_chunk_stream(&session)
            .await
            .expect("receive the chunk stream");
        assert_eq!(packets.len(), 3);
        assert!(matches!(
            &packets[0],
            ChunkPacketSummary::Column {
                x: 0,
                z: 0,
                heightmap_entries: 1024,
                ..
            }
        ));
        assert!(matches!(
            &packets[1],
            ChunkPacketSummary::Chunk {
                x: 4,
                y: 1,
                z: -5,
                data_bytes: 16,
                ..
            }
        ));
        assert_eq!(packets[2], ChunkPacketSummary::Unload { x: 4, z: -5 });

        session.connection().close(0u32.into(), b"done");
        server_task.await.expect("server task").expect("server");
    };

    tokio::time::timeout(Duration::from_secs(10), scenario)
        .await
        .expect("client chunk stream test timed out");
}
