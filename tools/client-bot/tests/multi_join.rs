//! Persistent listener loopback: concurrent sessions, disconnect and rejoin.
//!
//! Needs the base-game pack (world generation) and loopback UDP, so it is
//! `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test multi_join -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, TlsIdentity};
use hytale_protocol::io::Uuid;
use hytale_server::{serve_listener, GeneratorOptions, ServeOptions, SessionOptions};
use tokio::sync::oneshot;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_root(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-multi-join-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn two_clients_join_disconnect_and_rejoin_without_restart() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

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
            default_view_radius: 0,
            handshake_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(30),
            ..SessionOptions::default()
        },
    );
    options.view_radius = 0;
    options.max_connections = 4;
    options.bound_tx = Some(bound_tx);

    let server_task = tokio::spawn(serve_listener(options, async move {
        let _ = shutdown_rx.await;
    }));
    let server_addr = tokio::time::timeout(Duration::from_secs(10), bound_rx)
        .await
        .expect("bound address")
        .expect("bound sender");

    let first_identity = TlsIdentity::generate("multi-client-1").expect("client cert");
    let second_identity = TlsIdentity::generate("multi-client-2").expect("client cert");
    let first = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &first_identity,
    )
    .await
    .expect("connect first");
    let second = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &second_identity,
    )
    .await
    .expect("connect second");

    let first_options = DevJoinOptions {
        uuid: Uuid::new(1, 1),
        username: "FirstBot".to_string(),
        view_radius_chunks: 0,
    };
    let second_options = DevJoinOptions {
        uuid: Uuid::new(1, 2),
        username: "SecondBot".to_string(),
        view_radius_chunks: 0,
    };
    let (first_report, second_report) = tokio::join!(
        dev_join(&first, &first_options),
        dev_join(&second, &second_options)
    );
    let first_report = first_report.expect("first join");
    let second_report = second_report.expect("second join");
    for report in [&first_report, &second_report] {
        assert_eq!(report.chunks.len(), 21);
        assert!(report.game.join_world);
        assert!(report.game.player_list);
        assert!(report.game.self_spawn);
    }

    // Closing one session must not stop the listener.
    first.connection().close(0u32.into(), b"first bot done");
    tokio::time::sleep(Duration::from_millis(250)).await;

    // A new connection joins after the first disconnect, without a restart.
    let third_identity = TlsIdentity::generate("multi-client-3").expect("client cert");
    let third = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &third_identity,
    )
    .await
    .expect("connect third");
    let third_report = dev_join(
        &third,
        &DevJoinOptions {
            uuid: Uuid::new(1, 3),
            username: "ThirdBot".to_string(),
            view_radius_chunks: 0,
        },
    )
    .await
    .expect("third join");
    assert_eq!(third_report.chunks.len(), 21);

    second.connection().close(0u32.into(), b"second bot done");
    third.connection().close(0u32.into(), b"third bot done");
    drop(first);

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
    assert_eq!(report.peak_connections, 2, "two clients were concurrent");
    assert!(report
        .sessions
        .iter()
        .any(|session| session.username == "FirstBot"));
    assert!(report
        .sessions
        .iter()
        .any(|session| session.username == "SecondBot"));
    assert!(report
        .sessions
        .iter()
        .any(|session| session.username == "ThirdBot"));

    std::fs::remove_dir_all(universe_root).ok();
}
