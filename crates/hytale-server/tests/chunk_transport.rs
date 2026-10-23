//! Loopback test: a `ChunkPacketBatch` crosses a real `Chunks` QUIC stream.
//!
//! This is the seam the connection task uses: world-thread event → batch →
//! zstd frames → chunk stream → client decode. It is `#[ignore]`d like the
//! other loopback UDP tests.
//!
//! ```sh
//! cargo test -p hytale-server --test chunk_transport -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use hytale_net::{
    connect, ChunkStreamError, ChunkStreamReader, ChunkStreamWriter, ServerEndpointBuilder,
    TlsIdentity,
};
use hytale_world::chunk::column::ChunkColumn;
use hytale_world::chunk::stream::{ChunkPacketBatch, DEFAULT_COMPRESSION_LEVEL};

#[tokio::test]
#[ignore = "binds loopback UDP sockets; run explicitly with --ignored"]
async fn a_chunk_batch_round_trips_over_the_chunk_stream() {
    let scenario = async {
        let mut column = ChunkColumn::new(4, -5);
        column.set_block(1, 64, 1, 7, 0, 0).expect("set block");
        column.set_tint(2, 3, 0x1234_5678);
        let batch = ChunkPacketBatch::full(&column).expect("batch");
        let frames = batch
            .frames(DEFAULT_COMPRESSION_LEVEL)
            .expect("frame the batch");

        let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
        let server = ServerEndpointBuilder::new(bind_addr, server_identity)
            .bind()
            .expect("bind server");
        let server_addr = server.local_addr();
        let client_identity = TlsIdentity::generate("batch-client").expect("client cert");

        let server_task = tokio::spawn(async move {
            let accepted = server.accept().await?;
            let mut writer = ChunkStreamWriter::open(accepted.connection()).await?;
            writer
                .write_frames(frames.iter().map(Vec::as_slice))
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
            .expect("accept chunk stream");

        for packet in &batch.packets {
            let frame = reader
                .next_frame()
                .await
                .expect("read frame")
                .expect("frame");
            assert_eq!(frame.packet_id, packet.id);
            assert_eq!(frame.payload, packet.payload);
        }
        assert!(reader.next_frame().await.expect("end").is_none());

        session.connection().close(0u32.into(), b"done");
        server_task.await.expect("server task").expect("server");
    };

    tokio::time::timeout(Duration::from_secs(10), scenario)
        .await
        .expect("chunk transport test timed out");
}
