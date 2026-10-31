//! End-to-end regression for the release-client authenticated join.
//!
//! The session service is a loopback HTTP stub and the JWKS is a throwaway
//! Ed25519 key. The bot sends a real `Connect(identityToken)` and the server
//! performs the real `AuthGrant` → `AuthToken` → `ServerAuthToken` exchange,
//! including the `x509Fingerprint` binding to its own TLS certificate.
//!
//! Needs the base-game pack and loopback UDP:
//!
//! ```sh
//! cargo test -p client-bot --test live_auth_join -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use client_bot::{auth_join_with_tokens, AuthJoinOptions};
use hytale_auth::live::{LiveAuth, LiveAuthConfig};
use hytale_auth::{AuthKey, AuthService, SCOPE_CLIENT};
use hytale_net::{connect, TlsIdentity};
use hytale_protocol::io::Uuid;
use hytale_server::{serve_listener, AuthMode, GeneratorOptions, ServeOptions, SessionOptions};
use serde_json::{json, Value};
use tokio::sync::oneshot;

const ISSUER: &str = "https://sessions.test";
const SERVER_AUDIENCE: &str = "live-test-server";

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

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-live-auth-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn the_authenticated_join_uses_the_live_session_contract() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let service = MockService::start(HashMap::from([
        (
            "/server-join/auth-grant".to_string(),
            json!({"authorizationGrant": "grant-from-service"}).to_string(),
        ),
        (
            "/server-join/auth-token".to_string(),
            json!({"accessToken": "server-access-from-service"}).to_string(),
        ),
    ]));

    let key = AuthKey::generate().expect("test signing key");
    let jwks = json!({
        "keys": [{
            "kty": "OKP",
            "crv": "Ed25519",
            "kid": key.key_id(),
            "x": key.public_key_base64(),
        }]
    });
    let mut config = LiveAuthConfig::loopback(service.base_url());
    config.issuer = ISSUER.to_string();
    config.audience = Some(SERVER_AUDIENCE.to_string());
    let live = LiveAuth::with_jwks(config, "server-session-token", "server-identity", &jwks)
        .expect("live auth");
    let authority = AuthService::new(key, ISSUER, SERVER_AUDIENCE);

    let (bound_tx, bound_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mut options = ServeOptions::new(
        temp_dir("universe"),
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
            auth: AuthMode::authenticated(live),
            ..SessionOptions::default()
        },
    );
    options.view_radius = 0;
    options.max_connections = 2;
    options.bound_tx = Some(bound_tx);
    let server = tokio::spawn(serve_listener(options, async move {
        let _ = shutdown_rx.await;
    }));
    let server_addr = tokio::time::timeout(Duration::from_secs(10), bound_rx)
        .await
        .expect("bound address")
        .expect("bound sender");

    let client_identity = TlsIdentity::generate("live-auth-client").expect("client cert");
    let client_fingerprint = client_identity.fingerprint_base64url();
    let session = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &client_identity,
    )
    .await
    .expect("connect");

    let uuid = Uuid::new(0x0123_4567_89ab_cdef, 0x0123_4567_89ab_cdef);
    let subject = uuid.to_string();
    let identity_token = authority
        .issue_identity_token(
            &subject,
            "LivePlayer",
            &[SCOPE_CLIENT.to_string()],
            None,
            300,
        )
        .expect("identity token");
    let access_token = authority
        .issue_access_token(
            &subject,
            "LivePlayer",
            &[SCOPE_CLIENT.to_string()],
            Some(&client_fingerprint),
            300,
        )
        .expect("access token");

    let report = auth_join_with_tokens(
        &session,
        &AuthJoinOptions {
            uuid,
            username: "LivePlayer".to_string(),
            view_radius_chunks: 0,
            auth: authority,
            certificate_fingerprint: client_fingerprint,
            asset_limit: Some(1),
            movement_chunk: None,
            observe_time: None,
        },
        Some(identity_token.clone()),
        Some(access_token),
    )
    .await
    .expect("authenticated join");

    assert!(report.auth_grant, "server did not send AuthGrant");
    assert!(
        report.server_auth_token,
        "server did not send ServerAuthToken"
    );
    assert!(report.game.join_world, "JoinWorld not received");
    assert!(report.game.self_spawn, "self EntityUpdates not received");
    assert_eq!(report.chunks.len(), 21, "expected the radius-0 snapshot");

    let requests = service.requests();
    let grant = requests
        .iter()
        .find(|request| request.path == "/server-join/auth-grant")
        .expect("auth-grant request");
    assert_eq!(
        grant.authorization.as_deref(),
        Some("Bearer server-session-token")
    );
    let grant_body: Value = serde_json::from_str(&grant.body).expect("grant body");
    assert_eq!(grant_body["identityToken"], identity_token);
    assert_eq!(grant_body["aud"], SERVER_AUDIENCE);

    let token = requests
        .iter()
        .find(|request| request.path == "/server-join/auth-token")
        .expect("auth-token request");
    assert_eq!(
        token.authorization.as_deref(),
        Some("Bearer server-session-token")
    );
    let token_body: Value = serde_json::from_str(&token.body).expect("token body");
    assert_eq!(token_body["authorizationGrant"], "grant-from-service");
    let server_certificate = session
        .peer_identity()
        .expect("server certificate")
        .fingerprint_base64url();
    assert_eq!(
        token_body["x509Fingerprint"], server_certificate,
        "the grant exchange must bind the server's TLS certificate"
    );

    session.connection().close(0u32.into(), b"test complete");
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(30), server).await;
}

struct RecordedRequest {
    path: String,
    authorization: Option<String>,
    body: String,
}

struct MockService {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

impl MockService {
    fn start(routes: HashMap<String, String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock service");
        listener
            .set_nonblocking(true)
            .expect("nonblocking mock service");
        let address = listener.local_addr().expect("mock address");
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let routes = Arc::new(routes);
        let thread_stop = Arc::clone(&stop);
        let thread_requests = Arc::clone(&requests);
        std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if let Some((request, response)) = handle_request(&mut stream, &routes) {
                            thread_requests.lock().expect("requests").push(request);
                            let _ = stream.write_all(response.as_bytes());
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            stop,
            requests,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        let requests = self.requests.lock().expect("requests");
        requests
            .iter()
            .map(|request| RecordedRequest {
                path: request.path.clone(),
                authorization: request.authorization.clone(),
                body: request.body.clone(),
            })
            .collect()
    }
}

impl Drop for MockService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn handle_request(
    stream: &mut TcpStream,
    routes: &HashMap<String, String>,
) -> Option<(RecordedRequest, String)> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let header_end = buffer.windows(4).position(|window| window == b"\r\n\r\n")? + 4;
    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let request_line = headers.lines().next()?;
    let path = request_line.split_whitespace().nth(1)?.to_string();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let authorization = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.trim().to_string())
    });
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&body).to_string();
    let response_body = routes
        .get(&path)
        .cloned()
        .unwrap_or_else(|| json!({"error": "not found"}).to_string());
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
        response_body.len()
    );
    Some((
        RecordedRequest {
            path,
            authorization,
            body,
        },
        response,
    ))
}
