//! T72 concurrent load gate.
//!
//! Drives a cheap 10-bot run (override with `HYTALE_LOAD_PLAYERS`) against one
//! in-process listener and asserts the harness produces a complete report
//! without deadlocks, duplicate identities or queue overflows.
//!
//! ```sh
//! cargo test -p client-bot --test load -- --ignored
//! HYTALE_LOAD_PLAYERS=50 cargo test -p client-bot --test load -- --ignored --nocapture
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use client_bot::load::{run_load, LoadMemoryReport, LoadOptions, ServerLoadReport};
use hytale_server::{serve_listener, GeneratorOptions, ServeOptions, SessionOptions};
use tokio::sync::oneshot;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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
    std::env::temp_dir().join(format!("hytale-load-{}-{nonce}-{name}", std::process::id()))
}

fn requested_players() -> usize {
    std::env::var("HYTALE_LOAD_PLAYERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|players| *players > 0)
        .unwrap_or(10)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn concurrent_bots_join_one_listener_without_overflow() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let players = requested_players();
    let universe_root = temp_root("universe");
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
            // The session registry enforces `max_players`, not only the
            // listener semaphore, so size it for the run.
            max_players: i32::try_from(players + 4).unwrap_or(i32::MAX),
            default_view_radius: 0,
            handshake_timeout: Duration::from_secs(60),
            world_join_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
            ..SessionOptions::default()
        },
    );
    options.view_radius = 0;
    options.max_connections = players + 4;
    options.bound_tx = Some(bound_tx);

    let server_task = tokio::spawn(serve_listener(options, async move {
        let _ = shutdown_rx.await;
    }));
    // World startup dominates in debug builds (asset decode + spawn chunks).
    let server_addr = tokio::time::timeout(Duration::from_secs(180), bound_rx)
        .await
        .expect("bound address")
        .expect("bound sender");

    // The bigger runs need a longer overlap so early joiners stay connected
    // while the slower asset bursts finish; debug builds are CPU-bound.
    let workload_window = Duration::from_secs((players as u64 / 2).max(2));
    let started = Instant::now();
    let mut report = run_load(
        server_addr,
        "localhost",
        &LoadOptions {
            players,
            stagger: Duration::from_millis(25),
            workload_window,
            ..LoadOptions::default()
        },
    )
    .await;
    let elapsed = started.elapsed();

    let _ = shutdown_tx.send(());
    let serve_report = tokio::time::timeout(Duration::from_secs(120), server_task)
        .await
        .expect("listener shutdown deadline")
        .expect("listener task")
        .expect("listener report");
    report.server = Some(ServerLoadReport::from_serve(&serve_report, elapsed));
    report.memory = LoadMemoryReport {
        resident_bytes: hytale_server::resident_bytes(),
        peak_resident_bytes: hytale_server::peak_resident_bytes(),
    };

    let report_path = repo_root().join(format!("target/perf/load-test-{players}.json"));
    report
        .write_json(&report_path)
        .expect("write the load report");
    println!("{}", report.summary());
    println!("report: {}", report_path.display());

    assert_eq!(report.players, players);
    assert_eq!(report.joined, players, "all bots must join: {report:#?}");
    assert_eq!(report.failed, 0, "join failures: {report:#?}");
    assert_eq!(report.latency.samples, players);
    assert!(report.latency.p50_ms >= 0.0);
    assert!(report.latency.p95_ms >= report.latency.p50_ms);
    assert!(report.latency.max_ms >= report.latency.p95_ms);
    assert_eq!(
        report.max_players_seen, players,
        "at least one bot must see the full player list: {report:#?}"
    );

    let server = report.server.as_ref().expect("in-process server report");
    assert_eq!(server.accepted, players, "server sessions: {server:#?}");
    assert_eq!(
        server.rejected, 0,
        "listener rejected sessions: {server:#?}"
    );
    assert_eq!(
        server.duplicate_identities, 0,
        "duplicate logins: {server:#?}"
    );
    assert_eq!(
        server.queue_overflows, 0,
        "broadcast queue overflow: {server:#?}"
    );
    assert_eq!(server.rate_limited, 0, "rate limited: {server:#?}");
    assert_eq!(server.idle_timeouts, 0, "idle timeouts: {server:#?}");
    assert!(
        server.unexpected_errors.is_empty(),
        "unexpected session errors: {server:#?}"
    );
    assert!(server.chunk_packets > 0, "chunk packets: {server:#?}");
    assert!(server.chunk_frames >= server.chunk_packets);
    assert!(server.chunk_payload_bytes > 0, "chunk bytes: {server:#?}");
    assert!(server.chunks > 0, "chunks sent: {server:#?}");
    assert!(server.block_edits > 0, "block edits: {server:#?}");
    assert!(server.simulation_ticks > 0, "world ticks: {server:#?}");
    assert!(server.effective_tick_rate > 0.0);
    assert!(
        report.memory.resident_bytes.unwrap_or(0) > 0,
        "resident memory: {report:#?}"
    );
    assert!(
        report.memory.peak_resident_bytes.unwrap_or(0) >= report.memory.resident_bytes.unwrap_or(0)
    );

    let json = std::fs::read_to_string(&report_path).expect("read the load report");
    let decoded: client_bot::load::LoadReport =
        serde_json::from_str(&json).expect("decode the load report");
    assert_eq!(decoded.players, players);
    assert_eq!(decoded.joined, players);

    std::fs::remove_dir_all(universe_root).ok();
}
