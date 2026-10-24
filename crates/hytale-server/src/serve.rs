//! Persistent multi-session listener.
//!
//! One endpoint and one `UniverseRuntime` live for the process lifetime. Every
//! accepted connection gets an independent task running the existing join state
//! machine, with its own handshake, view and chunk stream. Sessions share the
//! selected world thread (and therefore its cached config burst); assets are not
//! re-encoded per connection.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hytale_net::{ServerEndpointBuilder, TlsIdentity, TransportError};
use thiserror::Error;
use tokio::runtime::Runtime;
use tokio::sync::{oneshot, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{timeout, Instant};

use crate::generation::GeneratorOptions;
use crate::players::PlayerRegistry;
use crate::session::{serve_connection_with_registry, SessionError, SessionOptions, SessionReport};
use crate::universe_runtime::{UniverseRuntime, UniverseRuntimeError};

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
    /// Reading the universe catalog failed.
    #[error(transparent)]
    Catalog(#[from] hytale_world::universe::UniverseError),
    /// QUIC setup/accept failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The selected world does not exist.
    #[error("world `{0}` is not in the universe")]
    MissingWorld(String),
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
        .with_view(options.view_radius, options.chunks_per_tick);

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

    let identity = TlsIdentity::generate("localhost")?;
    let server = ServerEndpointBuilder::new(options.bind_addr, identity).bind()?;
    let bound_addr = server.local_addr();
    if let Some(sender) = options.bound_tx {
        let _ = sender.send(bound_addr);
    }
    println!(
        "serve listening on {bound_addr} (world {} / {})",
        descriptor.name, descriptor.uuid
    );

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
            accepted = server.accept() => {
                let accepted = accepted?;
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
                let session = options.session.clone();
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

    let saved_chunks = runtime.shutdown()?;
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
        Ok(Err(error)) => errors.push(error.to_string()),
        Err(error) if !error.is_cancelled() => errors.push(format!("session task: {error}")),
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
