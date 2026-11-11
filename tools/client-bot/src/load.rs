//! Concurrent player load harness.
//!
//! The harness drives `dev_join` from N independent QUIC clients against one
//! listener, staggers the joins, runs a short move/chat/edit/save window and
//! reports join latency, chunk throughput, session health and process memory.
//! Server-side counters come from [`hytale_server::ServeReport`] when the caller
//! owns an in-process listener (the ignored `load` test does).

use std::collections::BTreeSet;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use hytale_net::{connect, TlsIdentity};
use hytale_protocol::generated::common::{BlockPosition, Position};
use hytale_protocol::io::Uuid;
use hytale_server::ServeReport;
use serde::{Deserialize, Serialize};

use crate::{dev_join, DevJoinOptions};

/// Deterministic UUID space for load bots (`LoadBot000`, `LoadBot001`, ...).
pub const LOAD_UUID_NAMESPACE: i64 = 0x4c4f_4144_424f_5400;

/// Options for one load run.
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Number of bots to join.
    pub players: usize,
    /// Delay between consecutive bot joins.
    pub stagger: Duration,
    /// Per-connection timeout for the handshake, asset burst and join snapshot.
    pub join_timeout: Duration,
    /// Requested view radius, in chunks.
    pub view_radius_chunks: u32,
    /// Post-join move/chat/edit/save observation window.
    pub workload_window: Duration,
    /// First bot position; the rest are laid out in a deterministic grid.
    pub base_position: Position,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            players: 10,
            stagger: Duration::from_millis(25),
            join_timeout: Duration::from_secs(60),
            view_radius_chunks: 0,
            workload_window: Duration::from_secs(2),
            // Keep the whole grid inside the spawn chunk so every bot edits a
            // chunk that is loaded by the join snapshot.
            base_position: Position {
                x: -31.0,
                y: 200.0,
                z: -31.0,
            },
        }
    }
}

/// Join-latency distribution in milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LatencySummary {
    /// Number of successful samples.
    pub samples: usize,
    /// Median join latency.
    pub p50_ms: f64,
    /// 95th percentile join latency.
    pub p95_ms: f64,
    /// Longest join latency.
    pub max_ms: f64,
}

impl LatencySummary {
    /// Build a summary from unsorted millisecond samples.
    pub fn from_samples(mut samples: Vec<f64>) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        samples.sort_by(f64::total_cmp);
        let percentile = |p: f64| {
            let index = ((samples.len() - 1) as f64 * p).ceil() as usize;
            samples[index.min(samples.len() - 1)]
        };
        Self {
            samples: samples.len(),
            p50_ms: percentile(0.50),
            p95_ms: percentile(0.95),
            max_ms: *samples.last().expect("non-empty samples"),
        }
    }
}

/// One bot's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadBotReport {
    /// Zero-based bot index.
    pub index: usize,
    /// Deterministic player UUID.
    pub uuid: String,
    /// Deterministic username.
    pub username: String,
    /// The join completed and the snapshot arrived.
    pub joined: bool,
    /// Failure detail when `joined` is false.
    pub error: Option<String>,
    /// Connect + handshake + world-join latency.
    pub join_latency_ms: Option<f64>,
    /// Chunk-stream packets in the join snapshot.
    pub chunks: usize,
    /// The initial player list arrived.
    pub player_list: bool,
    /// Unique player UUIDs seen in the initial list or later adds.
    pub players_seen: usize,
    /// `ServerMessage` lines observed during the workload window.
    pub server_messages: usize,
}

/// Server-side load counters (in-process listeners only).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerLoadReport {
    /// Served world name.
    pub world_name: String,
    /// Served world UUID.
    pub world_uuid: String,
    /// Finished sessions.
    pub accepted: usize,
    /// Connections refused by the listener semaphore.
    pub rejected: usize,
    /// Refused connections closed with the listener's `server full` reason.
    pub server_full: usize,
    /// Highest concurrent session count.
    pub peak_connections: usize,
    /// Every session task error, including client teardown races.
    pub errors: Vec<String>,
    /// Errors caused by the client dropping a stream/connection while the
    /// server was still writing (expected at the end of a load run).
    pub client_teardown_errors: usize,
    /// Errors that are not duplicate identities or client teardown.
    pub unexpected_errors: Vec<String>,
    /// Errors that match the duplicate-identity rejection.
    pub duplicate_identities: usize,
    /// Chunk-stream packets across sessions.
    pub chunk_packets: usize,
    /// Chunk-stream QUIC frames across sessions.
    pub chunk_frames: usize,
    /// Uncompressed chunk payload bytes across sessions.
    pub chunk_payload_bytes: usize,
    /// Chunks streamed across sessions.
    pub chunks: usize,
    /// Chunks unloaded across sessions.
    pub unloaded: usize,
    /// View changes across sessions.
    pub view_changes: usize,
    /// Block edits applied to the world.
    pub block_edits: usize,
    /// Block edits rejected.
    pub block_edit_rejections: usize,
    /// Block-use interactions handled.
    pub block_uses: usize,
    /// Plain chat lines relayed.
    pub chat_messages: usize,
    /// Chat command lines handled.
    pub commands: usize,
    /// Sessions disconnected by queue overflow.
    pub queue_overflows: usize,
    /// Sessions disconnected for exceeding the inbound rate.
    pub rate_limited: usize,
    /// Sessions disconnected for idling.
    pub idle_timeouts: usize,
    /// World tick/simulation passes across sessions.
    pub simulation_ticks: usize,
    /// Dirty chunks flushed during shutdown.
    pub saved_chunks: usize,
    /// Wall-clock time from the first connect to listener shutdown.
    pub elapsed_ms: f64,
    /// `simulation_ticks / elapsed` across all sessions.
    pub effective_tick_rate: f64,
}

impl ServerLoadReport {
    /// Summarize an in-process listener report.
    pub fn from_serve(report: &ServeReport, elapsed: Duration) -> Self {
        let sum = |value: fn(&hytale_server::SessionSummary) -> usize| {
            report.sessions.iter().map(value).sum()
        };
        let duplicate_identities = report
            .errors
            .iter()
            .filter(|error| error.contains("already connected"))
            .count();
        let client_teardown_errors = report
            .errors
            .iter()
            .filter(|error| is_client_teardown(error))
            .count();
        let unexpected_errors: Vec<String> = report
            .errors
            .iter()
            .filter(|error| !is_client_teardown(error) && !error.contains("already connected"))
            .cloned()
            .collect();
        // Sessions that returned a report plus sessions the client closed while
        // the server was writing. Both were accepted and joined the world.
        let accepted = report.sessions.len() + client_teardown_errors;
        let simulation_ticks = sum(|session| session.simulation_ticks);
        let elapsed_seconds = elapsed.as_secs_f64();
        Self {
            world_name: report.world_name.clone(),
            world_uuid: report.world_uuid.clone(),
            accepted,
            rejected: report.rejected,
            // The listener refuses at the semaphore with this close reason.
            server_full: report.rejected,
            peak_connections: report.peak_connections,
            errors: report.errors.clone(),
            client_teardown_errors,
            unexpected_errors,
            duplicate_identities,
            chunk_packets: sum(|session| session.packets),
            chunk_frames: sum(|session| session.frames),
            chunk_payload_bytes: sum(|session| session.payload_bytes),
            chunks: sum(|session| session.chunks),
            unloaded: sum(|session| session.unloaded),
            view_changes: sum(|session| session.view_changes),
            block_edits: sum(|session| session.block_edits),
            block_edit_rejections: sum(|session| session.block_edit_rejections),
            block_uses: sum(|session| session.block_uses),
            chat_messages: sum(|session| session.chat_messages),
            commands: sum(|session| session.commands),
            queue_overflows: report
                .sessions
                .iter()
                .filter(|session| session.queue_overflow)
                .count(),
            rate_limited: report
                .sessions
                .iter()
                .filter(|session| session.rate_limited)
                .count(),
            idle_timeouts: report
                .sessions
                .iter()
                .filter(|session| session.idle_timeout)
                .count(),
            simulation_ticks,
            saved_chunks: report.saved_chunks,
            elapsed_ms: elapsed_seconds * 1000.0,
            effective_tick_rate: if elapsed_seconds > 0.0 {
                simulation_ticks as f64 / elapsed_seconds
            } else {
                0.0
            },
        }
    }
}

/// Process memory samples.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadMemoryReport {
    /// Resident set after the run.
    pub resident_bytes: Option<u64>,
    /// High-water resident set after the run.
    pub peak_resident_bytes: Option<u64>,
}

/// One completed load run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadReport {
    /// Requested player count.
    pub players: usize,
    /// Bots that joined successfully.
    pub joined: usize,
    /// Bots that failed before the join completed.
    pub failed: usize,
    /// Wall-clock duration from the first connect to the last bot leaving.
    pub elapsed_ms: f64,
    /// Join-latency distribution.
    pub latency: LatencySummary,
    /// Highest number of distinct players any bot saw.
    pub max_players_seen: usize,
    /// Per-bot results, in request order.
    pub bots: Vec<LoadBotReport>,
    /// Server-side counters when the caller owns the listener.
    pub server: Option<ServerLoadReport>,
    /// Process memory after the run.
    pub memory: LoadMemoryReport,
}

impl LoadReport {
    /// Render the JSON report.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Write the JSON report to `path`, creating parent directories.
    pub fn write_json(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json()?)
    }

    /// One-line summary for stdout.
    pub fn summary(&self) -> String {
        format!(
            "load players={} joined={} failed={} latency p50={:.1}ms p95={:.1}ms max={:.1}ms \
             players_seen={} elapsed={:.1}ms",
            self.players,
            self.joined,
            self.failed,
            self.latency.p50_ms,
            self.latency.p95_ms,
            self.latency.max_ms,
            self.max_players_seen,
            self.elapsed_ms,
        )
    }
}

/// Deterministic per-bot identity.
pub fn bot_identity(index: usize) -> (Uuid, String) {
    let uuid = Uuid::new(LOAD_UUID_NAMESPACE, index as i64 + 1);
    (uuid, format!("LoadBot{index:03}"))
}

/// Deterministic bot position, spread over a 10x10 grid of one-block cells.
pub fn bot_position(base: &Position, index: usize) -> Position {
    let column = (index % 10) as f64;
    let row = (index / 10) as f64;
    Position {
        x: base.x + column,
        y: base.y,
        z: base.z + row,
    }
}

/// Join `options.players` bots against `server_addr`.
pub async fn run_load(
    server_addr: SocketAddr,
    server_name: &str,
    options: &LoadOptions,
) -> LoadReport {
    let started = Instant::now();
    let mut tasks = Vec::with_capacity(options.players);
    for index in 0..options.players {
        let options = options.clone();
        let server_name = server_name.to_string();
        tasks.push(tokio::spawn(async move {
            run_bot(server_addr, &server_name, &options, index).await
        }));
    }
    let mut bots = Vec::with_capacity(tasks.len());
    for task in tasks {
        match task.await {
            Ok(report) => bots.push(report),
            Err(error) => bots.push(LoadBotReport {
                index: bots.len(),
                uuid: String::new(),
                username: String::new(),
                joined: false,
                error: Some(format!("load task joined incorrectly: {error}")),
                join_latency_ms: None,
                chunks: 0,
                player_list: false,
                players_seen: 0,
                server_messages: 0,
            }),
        }
    }
    let elapsed = started.elapsed();
    let joined = bots.iter().filter(|bot| bot.joined).count();
    let failed = bots.len() - joined;
    let latency = LatencySummary::from_samples(
        bots.iter()
            .filter_map(|bot| bot.join_latency_ms)
            .collect::<Vec<_>>(),
    );
    let max_players_seen = bots.iter().map(|bot| bot.players_seen).max().unwrap_or(0);

    LoadReport {
        players: options.players,
        joined,
        failed,
        elapsed_ms: elapsed.as_secs_f64() * 1000.0,
        latency,
        max_players_seen,
        bots,
        server: None,
        // Only callers that host the listener in this process can sample its
        // resident set. External-server runs leave this empty.
        memory: LoadMemoryReport::default(),
    }
}

async fn run_bot(
    server_addr: SocketAddr,
    server_name: &str,
    options: &LoadOptions,
    index: usize,
) -> LoadBotReport {
    let (uuid, username) = bot_identity(index);
    let mut report = LoadBotReport {
        index,
        uuid: uuid.to_string(),
        username: username.clone(),
        joined: false,
        error: None,
        join_latency_ms: None,
        chunks: 0,
        player_list: false,
        players_seen: 0,
        server_messages: 0,
    };
    if !options.stagger.is_zero() {
        tokio::time::sleep(options.stagger * index as u32).await;
    }
    let position = bot_position(&options.base_position, index);
    let target = BlockPosition {
        x: position.x.floor() as i32,
        y: position.y.floor() as i32,
        z: position.z.floor() as i32,
    };
    let mut commands = vec!["load ready".to_string(), "/who".to_string()];
    if index == 0 {
        commands.push("/save".to_string());
    }
    let join_options = DevJoinOptions {
        uuid,
        username,
        view_radius_chunks: options.view_radius_chunks,
        skin: None,
        movement_after_join: Some(position.clone()),
        movement_after_edits: Some(Position {
            x: position.x + 1.0,
            y: position.y,
            z: position.z,
        }),
        observe_after_join: Some(options.workload_window),
        disconnect_after: options
            .workload_window
            .checked_sub(Duration::from_millis(200)),
        // Hold the streams in the background after the report so the server
        // can finish its in-flight writes instead of seeing STOP_SENDING.
        keep_alive: true,
        place_block_after_join: Some((target.clone(), "Rock_Stone".to_string())),
        break_block_after_join: Some(target.clone()),
        use_block_after_join: Vec::new(),
        send_ready_for_gameplay: true,
        commands_after_join: commands,
        place_mode_after_join: None,
        interaction_chains_after_join: Vec::new(),
        arg_value_requests_after_join: Vec::new(),
    };
    let identity = match TlsIdentity::generate(format!("load-bot-{index:03}")) {
        Ok(identity) => identity,
        Err(error) => {
            report.error = Some(format!("client certificate: {error}"));
            return report;
        }
    };
    let bind_addr = match server_addr {
        SocketAddr::V4(_) => SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        SocketAddr::V6(_) => SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
    };
    let started = Instant::now();
    let join = tokio::time::timeout(options.join_timeout, async {
        let connect_started = Instant::now();
        let session = connect(bind_addr, server_addr, server_name, &identity)
            .await
            .map_err(|error| error.to_string())?;
        let connect_elapsed = connect_started.elapsed();
        let joined = dev_join(&session, &join_options)
            .await
            .map_err(|error| error.to_string());
        // Let the server observe the client disconnect and close first; tearing
        // the connection down underneath a busy session shows up as a write
        // failure and inflates the listener's error count.
        let _ = tokio::time::timeout(Duration::from_secs(10), session.connection().closed()).await;
        session.connection().close(0u32.into(), b"load complete");
        joined.map(|report| (report, connect_elapsed))
    })
    .await;
    match join {
        Ok(Ok((result, connect_elapsed))) => {
            report.joined = true;
            report.join_latency_ms =
                Some((connect_elapsed + result.joined_after).as_secs_f64() * 1000.0);
            report.chunks = result.chunks.len();
            report.player_list = result.game.player_list;
            report.server_messages = result.observation.server_messages.len();
            let mut seen: BTreeSet<String> = result
                .game
                .player_list_uuids
                .into_iter()
                .map(|uuid| uuid.to_string())
                .collect();
            seen.extend(
                result
                    .observation
                    .added
                    .into_iter()
                    .map(|uuid| uuid.to_string()),
            );
            report.players_seen = seen.len();
        }
        Ok(Err(error)) => {
            report.error = Some(error);
            report.join_latency_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
        }
        Err(_) => {
            report.error = Some(format!(
                "join timed out after {:.1}s",
                options.join_timeout.as_secs_f64()
            ));
            report.join_latency_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    report
}

/// A load bot closes its connection without waiting for every queued write.
/// Quinn reports that race as `STOP_SENDING` or a lost connection; it is not a
/// server fault and must not fail the load gate.
fn is_client_teardown(error: &str) -> bool {
    error.contains("sending stopped by peer")
        || error.contains("connection lost")
        || error.contains("Connection lost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_percentiles_sort_and_clamp() {
        let latency = LatencySummary::from_samples(vec![30.0, 10.0, 20.0, 40.0, 50.0]);
        assert_eq!(latency.samples, 5);
        assert_eq!(latency.p50_ms, 30.0);
        assert_eq!(latency.p95_ms, 50.0);
        assert_eq!(latency.max_ms, 50.0);
        assert_eq!(
            LatencySummary::from_samples(Vec::new()),
            LatencySummary::default()
        );
    }

    #[test]
    fn client_teardown_errors_are_not_server_faults() {
        assert!(is_client_teardown(
            "stream write failed: sending stopped by peer: error 0"
        ));
        assert!(is_client_teardown("stream write failed: connection lost"));
        assert!(!is_client_teardown("player is already connected"));
        assert!(!is_client_teardown("world thread channel closed"));
    }

    #[test]
    fn bot_identities_and_positions_are_deterministic_and_distinct() {
        let (first_uuid, first_name) = bot_identity(0);
        let (again_uuid, again_name) = bot_identity(0);
        let (second_uuid, second_name) = bot_identity(1);
        assert_eq!(first_uuid, again_uuid);
        assert_eq!(first_name, again_name);
        assert_ne!(first_uuid, second_uuid);
        assert_ne!(first_name, second_name);
        assert_eq!(first_name, "LoadBot000");

        let base = Position {
            x: 0.0,
            y: 200.0,
            z: 0.0,
        };
        let first = bot_position(&base, 0);
        let second = bot_position(&base, 1);
        let eleventh = bot_position(&base, 10);
        assert_eq!(first, base);
        assert!(second.x > first.x);
        assert!(eleventh.z > first.z);
        assert_eq!(bot_position(&base, 0), first);
    }
}
