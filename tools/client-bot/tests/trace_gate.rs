//! V1 lifecycle trace gate: joins, moves, edits, disconnects and reconnects a
//! bot against the listener while the server records every packet.
//!
//! The committed fixture is a normalized server-side trace (ids, channels,
//! order, payload hashes). Dynamic payloads (time, entity transforms, player
//! lists) keep their packet/order/channel but not a hash. Run with
//! `HYTALE_UPDATE_TRACE=1` to regenerate the fixture after an intentional
//! protocol/lifecycle change.
//!
//! Needs the base-game pack and loopback UDP:
//!
//! ```sh
//! cargo test -p client-bot --test trace_gate -- --ignored
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{
    connect, read_trace, write_trace, TlsIdentity, TraceDirection, TraceFile, TraceRecord,
    TraceStream,
};
use hytale_protocol::generated::common::{BlockPosition, Position};
use hytale_protocol::io::Uuid;
use hytale_server::{serve_listener, GeneratorOptions, ServeOptions, SessionOptions};
use tokio::sync::oneshot;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path() -> PathBuf {
    repo_root().join("testdata/golden/server/trace/v1-lifecycle.jsonl")
}

fn assets_available() -> bool {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        return false;
    }
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_root(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-trace-gate-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn v1_lifecycle_trace_matches_the_committed_gate() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let universe_root = temp_root("universe");
    let trace_path = temp_root("capture").with_extension("jsonl");
    let (bound_tx, bound_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mut options = ServeOptions::new(
        &universe_root,
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        SessionOptions {
            default_view_radius: 0,
            handshake_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(30),
            ..SessionOptions::default()
        },
    );
    options.view_radius = 0;
    options.max_connections = 4;
    options.trace_path = Some(trace_path.clone());
    options.bound_tx = Some(bound_tx);

    let server_task = tokio::spawn(serve_listener(options, async move {
        let _ = shutdown_rx.await;
    }));
    let server_addr = tokio::time::timeout(Duration::from_secs(10), bound_rx)
        .await
        .expect("bound address")
        .expect("bound sender");

    let actor_identity = TlsIdentity::generate("trace-actor").expect("client cert");
    let observer_identity = TlsIdentity::generate("trace-observer").expect("client cert");
    let actor = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &actor_identity,
    )
    .await
    .expect("connect actor");
    let observer = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &observer_identity,
    )
    .await
    .expect("connect observer");

    let movement = Position {
        x: 4.0,
        y: 200.0,
        z: 4.0,
    };
    let target = BlockPosition { x: 4, y: 200, z: 4 };
    let actor_options = DevJoinOptions {
        commands_after_join: Vec::new(),
        uuid: Uuid::new(4, 1),
        username: "TraceActor".to_string(),
        view_radius_chunks: 0,
        skin: None,
        movement_after_join: Some(movement),
        movement_after_edits: None,
        observe_after_join: Some(Duration::from_secs(4)),
        disconnect_after: Some(Duration::from_millis(2_500)),
        keep_alive: false,
        use_block_after_join: Vec::new(),
        send_ready_for_gameplay: true,
        place_block_after_join: Some((target.clone(), "Rock_Stone".to_string())),
        break_block_after_join: None,
    };
    let observer_options = DevJoinOptions {
        commands_after_join: Vec::new(),
        uuid: Uuid::new(4, 2),
        username: "TraceObserver".to_string(),
        view_radius_chunks: 0,
        skin: None,
        movement_after_join: None,
        movement_after_edits: None,
        observe_after_join: Some(Duration::from_secs(3)),
        disconnect_after: None,
        keep_alive: true,
        use_block_after_join: Vec::new(),
        send_ready_for_gameplay: true,
        place_block_after_join: None,
        break_block_after_join: None,
    };
    let actor_future = dev_join(&actor, &actor_options);
    let observer_future = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        dev_join(&observer, &observer_options).await
    };
    let (actor_report, observer_report) = tokio::join!(actor_future, observer_future);
    actor_report.expect("actor join");
    observer_report.expect("observer join");

    // Reconnect the same identity and edit the block placed before the
    // disconnect. This exercises the saved transform and the server-side
    // teardown/reconnect path in the same trace.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let reconnect_identity = TlsIdentity::generate("trace-actor-again").expect("client cert");
    let reconnected = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &reconnect_identity,
    )
    .await
    .expect("reconnect actor");
    let reconnect_report = dev_join(
        &reconnected,
        &DevJoinOptions {
            commands_after_join: Vec::new(),
            uuid: Uuid::new(4, 1),
            username: "TraceActor".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(2)),
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
            place_block_after_join: None,
            break_block_after_join: Some(target),
        },
    )
    .await
    .expect("actor reconnect");
    assert_eq!(reconnect_report.chunks.len(), 21);

    observer.connection().close(0u32.into(), b"observer done");
    reconnected
        .connection()
        .close(0u32.into(), b"reconnect done");
    let _ = shutdown_tx.send(());
    let report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task")
        .expect("server report");
    assert!(
        report.errors.is_empty(),
        "session errors: {:?}",
        report.errors
    );
    assert_eq!(report.sessions.len(), 3);

    let trace = read_trace(&trace_path).expect("read packet trace");
    assert_eq!(trace.header.format, hytale_net::TRACE_FORMAT_VERSION);
    assert_eq!(trace.header.producer, "hytale-server");
    assert!(
        trace.records.iter().any(|record| {
            record.direction == TraceDirection::Outbound
                && record.packet_name.as_deref() == Some("ViewRadius")
                && record.stream == TraceStream::Chunks
                && record.channel.as_deref() == Some("Chunks")
        }),
        "the real client requires ViewRadius on the Chunks stream"
    );
    let block_updates = trace
        .records
        .iter()
        .filter(|record| record.packet_name.as_deref() == Some("ServerSetBlock"))
        .collect::<Vec<_>>();
    assert!(
        block_updates.len() >= 2,
        "expected the placement and reconnect break in the trace: {block_updates:?}"
    );
    assert!(block_updates
        .iter()
        .all(|record| record.stream == TraceStream::Chunks));

    let normalized = normalize(&trace);
    if std::env::var_os("HYTALE_UPDATE_TRACE").is_some() {
        write_trace(fixture_path(), &normalized).expect("write committed trace fixture");
        eprintln!(
            "updated {} ({} records)",
            fixture_path().display(),
            normalized.records.len()
        );
    } else {
        let expected = read_trace(fixture_path()).expect("read committed trace fixture");
        assert_eq!(expected.header.format, normalized.header.format);
        assert_trace_matches(&expected, &normalized);
    }

    std::fs::remove_dir_all(universe_root).ok();
    std::fs::remove_file(trace_path).ok();
}

/// Reduce a raw trace to the deterministic lifecycle gate:
/// outbound records only, one representative per packet type per session except
/// block updates (place and break), and hashes only for stable payloads.
fn normalize(trace: &TraceFile) -> TraceFile {
    let mut seen = BTreeSet::new();
    let records = trace
        .records
        .iter()
        .filter(|record| record.direction == TraceDirection::Outbound)
        .filter_map(|record| {
            let name = record.packet_name.as_deref()?;
            if matches!(name, "Pong" | "ServerDisconnect") {
                return None;
            }
            if name != "ServerSetBlock" && !seen.insert((record.session, name.to_string())) {
                return None;
            }
            let mut record = record.clone();
            if !stable_hash(name) {
                record.payload_sha256.clear();
            }
            Some(record)
        })
        .collect();
    TraceFile {
        header: trace.header.clone(),
        records,
    }
}

fn stable_hash(name: &str) -> bool {
    !matches!(
        name,
        "UpdateTime" | "EntityUpdates" | "AddToServerPlayerList" | "RemoveFromServerPlayerList"
    )
}

fn assert_trace_matches(expected: &TraceFile, actual: &TraceFile) {
    let expected = by_session(expected);
    let actual = by_session(actual);
    for (session, expected_records) in &expected {
        let actual_records = actual.get(session).unwrap_or_else(|| {
            panic!("session {session} is missing from the live trace");
        });
        let mut cursor = 0usize;
        for expected_record in expected_records {
            let found = actual_records[cursor..]
                .iter()
                .position(|actual_record| record_matches(expected_record, actual_record));
            let Some(offset) = found else {
                panic!(
                    "session {session}: missing {:?} (#{}) after record {} in {:?}",
                    expected_record.packet_name,
                    expected_record.packet_id,
                    cursor,
                    actual_records
                        .iter()
                        .map(|record| (&record.packet_name, record.packet_id))
                        .collect::<Vec<_>>()
                );
            };
            cursor += offset + 1;
        }
    }
}

fn by_session(trace: &TraceFile) -> BTreeMap<u32, Vec<&TraceRecord>> {
    let mut sessions: BTreeMap<u32, Vec<&TraceRecord>> = BTreeMap::new();
    for record in &trace.records {
        sessions.entry(record.session).or_default().push(record);
    }
    sessions
}

fn record_matches(expected: &TraceRecord, actual: &TraceRecord) -> bool {
    expected.stream == actual.stream
        && expected.direction == actual.direction
        && expected.packet_id == actual.packet_id
        && expected.packet_name == actual.packet_name
        && expected.channel == actual.channel
        && (expected.payload_sha256.is_empty() || expected.payload_sha256 == actual.payload_sha256)
}
