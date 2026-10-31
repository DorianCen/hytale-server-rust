//! Persistent multi-session listener.
//!
//! One endpoint and one `UniverseRuntime` live for the process lifetime. Every
//! accepted connection gets an independent task running the existing join state
//! machine, with its own handshake, view and chunk stream. Sessions share the
//! selected world thread (and therefore its cached config burst); assets are not
//! re-encoded per connection.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hytale_net::{ServerEndpointBuilder, TlsIdentity, TraceRecorder, TransportError};
use thiserror::Error;
use tokio::runtime::Runtime;
use tokio::sync::{oneshot, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{timeout, Instant};

use crate::generation::GeneratorOptions;
use crate::players::PlayerRegistry;
use crate::session::{serve_connection_with_registry, SessionError, SessionOptions, SessionReport};
use crate::universe_runtime::{
    PortalArrival, PortalTransferReport, UniverseRuntime, UniverseRuntimeError,
};

/// Session-facing portal router. It owns the universe runtime lock so a live
/// session can create/select an instance world and move its player while the
/// listener keeps accepting connections.
#[derive(Debug, Clone)]
pub struct PortalRouter {
    runtime: Arc<Mutex<UniverseRuntime>>,
}

impl PortalRouter {
    /// Build a router over a shared universe runtime.
    pub fn new(runtime: Arc<Mutex<UniverseRuntime>>) -> Self {
        Self { runtime }
    }

    /// Create/select the target instance world for a portal.
    pub fn open_portal_instance(
        &self,
        portal_key: &str,
        fallback: hytale_world::player_data::PlayerTransform,
    ) -> Result<(PortalArrival, Arc<crate::world_thread::WorldThread>), UniverseRuntimeError> {
        self.runtime
            .lock()
            .expect("universe runtime poisoned")
            .open_portal_instance(portal_key, fallback)
    }

    /// Persist/move the live player document into the target world.
    #[allow(clippy::too_many_arguments)]
    pub fn transfer_live_player(
        &self,
        source: &crate::world_thread::WorldThread,
        target: &crate::world_thread::WorldThread,
        player_uuid: hytale_world::world_config::WorldUuid,
        transform: hytale_world::player_data::PlayerTransform,
        inventory: hytale_inventory::PlayerInventory,
        health: Option<f32>,
        arrival: &PortalArrival,
    ) -> Result<PortalTransferReport, UniverseRuntimeError> {
        self.runtime
            .lock()
            .expect("universe runtime poisoned")
            .transfer_live_player(
                source,
                target,
                player_uuid,
                transform,
                inventory,
                health,
                arrival,
            )
    }
}

/// Options for the persistent listener.
#[derive(Debug)]
pub struct ServeOptions {
    /// Universe root containing `universe.json` and one directory per world.
    pub universe_root: PathBuf,
    /// World name/UUID to serve; defaults to the universe default.
    pub world: Option<String>,
    /// Generator options for a world that has to start.
    pub generator: GeneratorOptions,
    /// QUIC bind address (port 0 selects an OS port).
    pub bind_addr: SocketAddr,
    /// Concurrent session limit.
    pub max_connections: usize,
    /// View radius for started worlds.
    pub view_radius: u32,
    /// Maximum chunks loaded/generated per world-thread tick.
    pub chunks_per_tick: usize,
    /// Session handshake/game settings.
    pub session: SessionOptions,
    /// How long to wait for sessions during shutdown before aborting them.
    pub shutdown_timeout: Duration,
    /// How long to wait for a connection to close after its session ends.
    pub close_timeout: Duration,
    /// Optional NPC role spawned at the world spawn on startup.
    pub spawn_npc: Option<String>,
    /// Discover loaded environments and maintain their configured NPC density.
    pub auto_population: bool,
    /// Enabled builtin modules.
    pub builtin: hytale_builtin::BuiltinRuntime,
    /// Optional JSONL packet trace written for every accepted session.
    pub trace_path: Option<PathBuf>,
    /// Optional test/CLI hook that receives the actual bound address.
    pub bound_tx: Option<oneshot::Sender<SocketAddr>>,
}

impl ServeOptions {
    /// Build listener options with sensible defaults and no bound-address hook.
    pub fn new(
        universe_root: impl Into<PathBuf>,
        generator: GeneratorOptions,
        bind_addr: SocketAddr,
        session: SessionOptions,
    ) -> Self {
        Self {
            universe_root: universe_root.into(),
            world: None,
            generator,
            bind_addr,
            max_connections: 20,
            view_radius: 0,
            chunks_per_tick: 1,
            session,
            shutdown_timeout: Duration::from_secs(10),
            close_timeout: Duration::from_secs(5),
            spawn_npc: None,
            auto_population: false,
            builtin: hytale_builtin::BuiltinRuntime::compiled(),
            trace_path: None,
            bound_tx: None,
        }
    }
}

/// One finished session summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// Player username.
    pub username: String,
    /// `insecure` or `local`.
    pub auth_mode: &'static str,
    /// Chunks streamed.
    pub chunks: usize,
    /// Chunks unloaded.
    pub unloaded: usize,
    /// The client disconnected cleanly.
    pub disconnected: bool,
    /// The server disconnected the client for idleness.
    pub idle_timeout: bool,
    /// `ClientMovement` packets processed.
    pub movement_packets: usize,
    /// Movement packets refused by the finite/world bounds.
    pub movement_rejections: usize,
    /// Movement targets corrected by collision.
    pub movement_corrections: usize,
    /// Movement corrections that auto-stepped onto a block.
    pub movement_steps: usize,
    /// Block-simulation passes run.
    pub simulation_ticks: usize,
    /// Block changes emitted by the simulation.
    pub simulation_block_changes: usize,
    /// Fluid changes emitted by the simulation.
    pub simulation_fluid_changes: usize,
    /// NPCs spawned by the automatic population tick.
    pub npc_spawns: usize,
    /// NPC actors advanced by the world tick.
    pub npc_ticks: usize,
    /// NPC actors that moved.
    pub npc_moves: usize,
    /// NPC melee attacks produced.
    pub npc_attacks: usize,
    /// NPC actors that died.
    pub npc_deaths: usize,
    /// Target world after a live portal handoff.
    pub portal_world: Option<String>,
}

impl From<&SessionReport> for SessionSummary {
    fn from(report: &SessionReport) -> Self {
        Self {
            username: report.username.clone(),
            auth_mode: report.auth_mode,
            chunks: report.stream.chunks + report.game_loop.chunks,
            unloaded: report.stream.unloaded + report.game_loop.unloaded,
            disconnected: report.game_loop.disconnected,
            idle_timeout: report.game_loop.idle_timeout,
            movement_packets: report.game_loop.movement_packets,
            movement_rejections: report.game_loop.movement_rejections,
            movement_corrections: report.game_loop.movement_corrections,
            movement_steps: report.game_loop.movement_steps,
            simulation_ticks: report.game_loop.simulation_ticks,
            simulation_block_changes: report.game_loop.simulation_block_changes,
            simulation_fluid_changes: report.game_loop.simulation_fluid_changes,
            npc_spawns: report.game_loop.npc_spawns,
            npc_ticks: report.game_loop.npc_ticks,
            npc_moves: report.game_loop.npc_moves,
            npc_attacks: report.game_loop.npc_attacks,
            npc_deaths: report.game_loop.npc_deaths,
            portal_world: report.game_loop.portal_world.clone(),
        }
    }
}

/// What the persistent listener produced after shutdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeReport {
    /// Actual bound address.
    pub bound_addr: SocketAddr,
    /// Served world name.
    pub world_name: String,
    /// Served world UUID.
    pub world_uuid: String,
    /// Finished sessions.
    pub sessions: Vec<SessionSummary>,
    /// Session task errors (the listener itself stayed up).
    pub errors: Vec<String>,
    /// Connections rejected because the limit was reached.
    pub rejected: usize,
    /// Highest concurrent session count observed.
    pub peak_connections: usize,
    /// Dirty chunks flushed during shutdown.
    pub saved_chunks: usize,
}

/// Listener errors.
#[derive(Debug, Error)]
pub enum ServeError {
    /// The tokio runtime could not be created.
    #[error("could not create the tokio runtime: {0}")]
    Runtime(#[from] std::io::Error),
    /// Universe/world startup or shutdown failed.
    #[error(transparent)]
    Universe(#[from] UniverseRuntimeError),
    /// A world-thread operation failed.
    #[error(transparent)]
    Thread(#[from] crate::world_thread::WorldThreadError),
    /// Reading the universe catalog failed.
    #[error(transparent)]
    Catalog(#[from] hytale_world::universe::UniverseError),
    /// QUIC setup/accept failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The packet trace file could not be created.
    #[error("packet trace `{path}`: {source}")]
    Trace {
        /// Requested trace path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The selected world does not exist.
    #[error("world `{0}` is not in the universe")]
    MissingWorld(String),
}

/// The other loopback address family on the same port.
///
/// `localhost` resolves to `::1` first on many systems while the default
/// listener binds `127.0.0.1`. Without this companion a client dialing
/// `localhost` sends its QUIC Initial packets to a port where nothing is
/// listening, which surfaces as a 20-second "QUIC handshake failed" timeout
/// with no server-side log line.
fn loopback_companion(bind: SocketAddr) -> Option<SocketAddr> {
    // A port of 0 gets a different OS-assigned port per family; the caller
    // cannot publish both, and tests rely on a single ephemeral endpoint.
    if bind.port() == 0 {
        return None;
    }
    match bind.ip() {
        IpAddr::V4(ip) if ip.is_loopback() => Some(SocketAddr::new(
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            bind.port(),
        )),
        IpAddr::V6(ip) if ip.is_loopback() => Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            bind.port(),
        )),
        _ => None,
    }
}

/// Run the listener on a fresh tokio runtime until Ctrl-C.
pub fn run(options: ServeOptions) -> Result<ServeReport, ServeError> {
    let runtime = Runtime::new()?;
    runtime.block_on(serve(options, async {
        let _ = tokio::signal::ctrl_c().await;
    }))
}

/// Run the listener until `shutdown` resolves.
pub async fn serve<F>(options: ServeOptions, shutdown: F) -> Result<ServeReport, ServeError>
where
    F: std::future::Future<Output = ()>,
{
    let mut runtime = UniverseRuntime::open(&options.universe_root, options.generator.clone())?
        .with_view(options.view_radius, options.chunks_per_tick)
        .with_builtin(options.builtin);

    let world_name = match options.world.clone() {
        Some(world) => world,
        None => match runtime.universe().default_world() {
            Some(descriptor) => descriptor.name.clone(),
            None => {
                let descriptor = runtime.create_world("default", 0, Default::default())?;
                descriptor.name
            }
        },
    };
    if runtime.universe().resolve(&world_name).is_err() {
        return Err(ServeError::MissingWorld(world_name));
    }
    let descriptor = runtime.universe().resolve(&world_name)?.clone();
    runtime.start_world(&world_name)?;
    let thread = runtime.world_thread(&world_name)?;
    let thread_uuid = descriptor.uuid;
    if options.auto_population {
        thread.set_npc_auto_population(true)?;
    }
    if let Some(role) = &options.spawn_npc {
        let transform = thread
            .player_transform()?
            .unwrap_or_else(|| hytale_world::runtime::transform([0.0, 64.0, 0.0], [0.0, 0.0, 0.0]));
        let entity = thread.spawn_npc(role, transform)?;
        println!(
            "spawned NPC `{role}` as runtime entity {} in world {}",
            entity.id, descriptor.name
        );
    }

    let mut session_options = options.session.clone();
    let runtime = Arc::new(Mutex::new(runtime));
    session_options.portal_router = Some(Arc::new(PortalRouter::new(Arc::clone(&runtime))));
    if let Some(path) = &options.trace_path {
        let recorder =
            TraceRecorder::create(path, "hytale-server").map_err(|source| ServeError::Trace {
                path: path.clone(),
                source,
            })?;
        println!("packet trace: {}", path.display());
        session_options.trace = Some(recorder);
    }

    let identity = TlsIdentity::generate("localhost")?;
    let mut bind_addrs = vec![options.bind_addr];
    bind_addrs.extend(loopback_companion(options.bind_addr));
    let primary = bind_addrs.remove(0);
    let server = ServerEndpointBuilder::new(primary, identity).bind_addrs(bind_addrs.clone())?;
    let bound_addr = server.local_addr();
    if let Some(sender) = options.bound_tx {
        let _ = sender.send(bound_addr);
    }
    for (addr, error) in server.skipped() {
        eprintln!("warning: could not also listen on {addr}: {error}");
    }
    let listening = server
        .local_addrs()
        .map(|addr| addr.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "serve listening on {listening} (world {} / {})",
        descriptor.name, descriptor.uuid
    );
    if options.bind_addr.is_ipv4() && options.bind_addr.ip().is_unspecified() {
        println!(
            "note: {} is IPv4-only; use --bind [::]:{} to accept IPv6 clients (including a `localhost` that resolves to ::1)",
            options.bind_addr,
            options.bind_addr.port()
        );
    }

    let semaphore = Arc::new(Semaphore::new(options.max_connections.max(1)));
    let players = Arc::new(PlayerRegistry::new());
    let mut sessions: JoinSet<Result<SessionReport, SessionError>> = JoinSet::new();
    let mut summaries = Vec::new();
    let mut errors = Vec::new();
    let mut rejected = 0usize;
    let mut peak_connections = 0usize;
    let mut connections: Vec<quinn::Connection> = Vec::new();

    let mut shutdown = Box::pin(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            // Collect sessions as they finish so a failure is logged even when
            // no further connection arrives.
            result = sessions.join_next(), if !sessions.is_empty() => {
                if let Some(result) = result {
                    collect_session(result, &mut summaries, &mut errors);
                }
            }
            accepted = server.accept() => {
                let accepted = match accepted {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        eprintln!("QUIC handshake failed: {error}");
                        continue;
                    }
                };
                connections.retain(|connection| connection.close_reason().is_none());
                let permit = match Arc::clone(&semaphore).try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        rejected += 1;
                        accepted.connection().close(0u32.into(), b"server full");
                        continue;
                    }
                };
                peak_connections = peak_connections.max(
                    options.max_connections.max(1) - semaphore.available_permits(),
                );
                let thread = Arc::clone(&thread);
                let session = session_options.clone();
                let players = Arc::clone(&players);
                let connection = accepted.connection().clone();
                connections.push(connection.clone());
                let close_timeout = options.close_timeout;
                sessions.spawn(async move {
                    let _permit = permit;
                    let result =
                        serve_connection_with_registry(&connection, &thread, &session, players)
                            .await;
                    connection.close(0u32.into(), b"session complete");
                    let _ = timeout(close_timeout, connection.closed()).await;
                    result
                });
            }
        }
        while let Some(result) = sessions.try_join_next() {
            collect_session(result, &mut summaries, &mut errors);
        }
    }

    server.close();
    for connection in &connections {
        connection.close(0u32.into(), b"server shutdown");
    }
    let deadline = Instant::now() + options.shutdown_timeout;
    loop {
        if sessions.is_empty() {
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match timeout(remaining, sessions.join_next()).await {
            Ok(Some(result)) => collect_session(result, &mut summaries, &mut errors),
            Ok(None) => break,
            Err(_) => break,
        }
    }
    let aborted = sessions.len();
    if aborted > 0 {
        sessions.abort_all();
        while sessions.join_next().await.is_some() {}
    }
    drop(thread);

    let saved_chunks = runtime
        .lock()
        .expect("universe runtime poisoned")
        .shutdown()?;
    if let Some(trace) = &session_options.trace {
        let _ = trace.flush();
    }
    summaries.sort_by(|left, right| left.username.cmp(&right.username));

    Ok(ServeReport {
        bound_addr,
        world_name: descriptor.name,
        world_uuid: thread_uuid.to_string(),
        sessions: summaries,
        errors,
        rejected,
        peak_connections,
        saved_chunks,
    })
}

fn collect_session(
    result: Result<Result<SessionReport, SessionError>, tokio::task::JoinError>,
    summaries: &mut Vec<SessionSummary>,
    errors: &mut Vec<String>,
) {
    match result {
        Ok(Ok(report)) => summaries.push(SessionSummary::from(&report)),
        Ok(Err(error)) => {
            eprintln!("session error: {error}");
            errors.push(error.to_string());
        }
        Err(error) if !error.is_cancelled() => {
            eprintln!("session task: {error}");
            errors.push(format!("session task: {error}"));
        }
        Err(_) => {}
    }
}

/// A debug helper used by the loopback tests: run the listener with a oneshot
/// shutdown sender.
pub fn spawn(
    options: ServeOptions,
    shutdown: oneshot::Receiver<()>,
) -> tokio::task::JoinHandle<Result<ServeReport, ServeError>> {
    tokio::spawn(serve(options, async move {
        let _ = shutdown.await;
    }))
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

    use super::loopback_companion;

    #[test]
    fn loopback_listeners_cover_both_address_families() {
        assert_eq!(
            loopback_companion(SocketAddr::from((Ipv4Addr::LOCALHOST, 5520))),
            Some(SocketAddr::from((Ipv6Addr::LOCALHOST, 5520)))
        );
        assert_eq!(
            loopback_companion(SocketAddr::from((Ipv6Addr::LOCALHOST, 5520))),
            Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 5520)))
        );
    }

    #[test]
    fn only_loopback_addresses_get_a_companion() {
        assert_eq!(
            loopback_companion(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
            None,
            "an ephemeral port cannot be shared with a second family"
        );
        assert_eq!(
            loopback_companion(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 5520))),
            None,
            "wildcard binds are left to the caller's --bind choice"
        );
        assert_eq!(
            loopback_companion(SocketAddr::from(([192, 0, 2, 10], 5520))),
            None
        );
    }
}
