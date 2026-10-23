//! Loopback test for the `Chunks` framed packet stream.
//!
//! Like `quic_spike`, this binds loopback UDP and is `#[ignore]`d for the
//! default suite. Run it explicitly:
//!
//! ```sh
//! cargo test -p hytale-net --test chunk_stream -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use hytale_net::{
    connect, ChunkStreamError, ChunkStreamReader, ChunkStreamWriter, ServerEndpointBuilder,
    TlsIdentity,
};
use hytale_protocol::frame::{compress_payload, encode_frame};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::generated::world::{SetChunk, SetColumn};
use hytale_protocol::io::{Reader, Writer};

fn frame<T: Packet + Wire>(packet: &T) -> (Vec<u8>, Vec<u8>) {
    let mut writer = Writer::new();
    packet.encode(&mut writer).expect("encode payload");
    let payload = writer.into_inner();
    let wire = if T::COMPRESSED {
        compress_payload(&payload, 3).expect("compress")
    } else {
        payload.clone()
    };
    let mut frame = Vec::new();
    encode_frame(T::ID, &wire, &mut frame).expect("frame");
    (frame, payload)
}

#[tokio::test]
#[ignore = "binds loopback UDP sockets; run explicitly with --ignored"]
async fn chunk_frames_round_trip_over_a_uni_stream() {
    let scenario = async {
        let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
        let server = ServerEndpointBuilder::new(bind_addr, server_identity)
            .bind()
            .expect("bind server");
        let server_addr = server.local_addr();
        let client_identity = TlsIdentity::generate("chunk-client").expect("client cert");

        let (column_frame, column_payload) = frame(&SetColumn::default());
        let chunk = SetChunk {
            x: 4,
            y: 3,
            z: -5,
            local_light: Some(vec![7; 5]),
            data: Some(vec![1, 2, 3]),
            ..SetChunk::default()
        };
        let (chunk_frame, chunk_payload) = frame(&chunk);

        let server_task = tokio::spawn(async move {
            let accepted = server.accept().await?;
            let mut writer = ChunkStreamWriter::open(accepted.connection()).await?;
            writer
                .write_frames([column_frame.as_slice(), chunk_frame.as_slice()])
                .await?;
            writer.finish()?;
            let _ = accepted.connection().closed().await;
            Ok::<_, ChunkStreamError>(())
        });

        let session = connect(bind_addr, server_addr, "localhost", &client_identity)
            .await
            .expect("connect");
        let mut reader = ChunkStreamReader::accept(session.connection())
            .await
            .expect("accept the chunk stream");

        let first = reader
            .next_frame()
            .await
            .expect("first frame")
            .expect("frame");
        assert_eq!(first.packet_id, SetColumn::ID);
        assert_eq!(first.payload, column_payload);
        let _ = SetColumn::decode(&mut Reader::new(&first.payload)).expect("decode column");

        let second = reader
            .next_frame()
            .await
            .expect("second frame")
            .expect("frame");
        assert_eq!(second.packet_id, SetChunk::ID);
        assert_eq!(second.payload, chunk_payload);
        let decoded = SetChunk::decode(&mut Reader::new(&second.payload)).expect("decode chunk");
        assert_eq!(decoded.x, 4);
        assert_eq!(decoded.y, 3);
        assert_eq!(decoded.z, -5);
        assert_eq!(decoded.data, Some(vec![1, 2, 3]));

        assert!(reader.next_frame().await.expect("end of stream").is_none());

        session.connection().close(0u32.into(), b"done");
        server_task.await.expect("server task").expect("server");
    };

    tokio::time::timeout(Duration::from_secs(10), scenario)
        .await
        .expect("chunk stream test timed out");
}
