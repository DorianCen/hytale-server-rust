//! Regression tests for the live Hytale session-service authority.
//!
//! The session service is replaced by a loopback HTTP stub and the JWKS by a
//! throwaway Ed25519 key, so these tests exercise the real claim rules,
//! signature verification, mTLS certificate binding and request shapes without
//! touching the live Hytale services.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hytale_auth::live::{keys_from_jwks, LiveAuth, LiveAuthConfig, SessionService};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const ISSUER: &str = "https://sessions.test";
const CLIENT_CERT: &[u8] = b"client-certificate-der";

struct TestKey {
    pair: Ed25519KeyPair,
}

impl TestKey {
    fn generate() -> Self {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("key");
        Self {
            pair: Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("pkcs8"),
        }
    }

    fn jwks(&self) -> Value {
        json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "kid": "test-key",
                "x": encode(self.pair.public_key().as_ref()),
            }]
        })
    }

    fn mint(&self, claims: Value) -> String {
        let header = json!({"alg": "EdDSA", "typ": "JWT", "kid": "test-key"});
        let header = encode(&serde_json::to_vec(&header).expect("header"));
        let payload = encode(&serde_json::to_vec(&claims).expect("claims"));
        let signing_input = format!("{header}.{payload}");
        let signature = self.pair.sign(signing_input.as_bytes());
        format!("{header}.{payload}.{}", encode(signature.as_ref()))
    }

    fn identity(&self, scope: &str, expires_in: i64) -> String {
        self.mint(json!({
            "iss": ISSUER,
            "sub": "12345678-1234-5678-9abc-def012345678",
            "profile": {"username": "TestPlayer"},
            "scope": scope,
            "iat": now(),
            "exp": now() + expires_in,
        }))
    }

    fn access(&self, audience: &str, fingerprint: &str, expires_in: i64) -> String {
        self.mint(json!({
            "iss": ISSUER,
            "sub": "12345678-1234-5678-9abc-def012345678",
            "username": "TestPlayer",
            "aud": audience,
            "cnf": {"x5t#S256": fingerprint},
            "iat": now(),
            "exp": now() + expires_in,
        }))
    }
}

fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64
}

fn client_fingerprint() -> String {
    encode(Sha256::digest(CLIENT_CERT).as_slice())
}

fn live_auth(key: &TestKey, base_url: &str, audience: &str) -> Arc<LiveAuth> {
    let mut config = LiveAuthConfig::loopback(base_url);
    config.issuer = ISSUER.to_string();
    config.audience = Some(audience.to_string());
    LiveAuth::with_jwks(config, "session-token", "server-identity", &key.jwks()).expect("live auth")
}

#[test]
fn identity_token_is_validated_against_the_session_jwks() {
    let key = TestKey::generate();
    let auth = live_auth(&key, "http://127.0.0.1:9", "server-audience");
    let token = key.identity("hytale:client", 300);
    let claims = auth
        .validate_identity_token(&token, "hytale:client")
        .expect("identity token");
    assert_eq!(claims.subject, "12345678-1234-5678-9abc-def012345678");
    assert_eq!(claims.username, "TestPlayer");
    assert_eq!(claims.issuer, ISSUER);
}

#[test]
fn identity_token_rejects_wrong_issuer_scope_expiry_and_signature() {
    let key = TestKey::generate();
    let auth = live_auth(&key, "http://127.0.0.1:9", "server-audience");

    let no_scope = key.identity("hytale:server", 300);
    assert!(auth
        .validate_identity_token(&no_scope, "hytale:client")
        .is_err());

    let expired = key.identity("hytale:client", -120);
    assert!(auth
        .validate_identity_token(&expired, "hytale:client")
        .is_err());

    let wrong_issuer = key.mint(json!({
        "iss": "https://evil.test",
        "sub": "12345678-1234-5678-9abc-def012345678",
        "profile": {"username": "TestPlayer"},
        "scope": "hytale:client",
        "exp": now() + 300,
    }));
    assert!(auth
        .validate_identity_token(&wrong_issuer, "hytale:client")
        .is_err());

    let mut tampered = key.identity("hytale:client", 300);
    let last = tampered.pop().expect("signature char");
    tampered.push(if last == 'A' { 'B' } else { 'A' });
    assert!(auth
        .validate_identity_token(&tampered, "hytale:client")
        .is_err());
}

#[test]
fn access_token_requires_the_server_audience_and_client_certificate_binding() {
    let key = TestKey::generate();
    let auth = live_auth(&key, "http://127.0.0.1:9", "server-audience");
    let fingerprint = client_fingerprint();
    let valid = key.access("server-audience", &fingerprint, 300);
    let claims = auth
        .validate_access_token(&valid, &fingerprint)
        .expect("access token");
    assert_eq!(claims.username, "TestPlayer");

    let wrong_audience = key.access("another-server", &fingerprint, 300);
    assert!(auth
        .validate_access_token(&wrong_audience, &fingerprint)
        .is_err());

    let wrong_fingerprint = key.access("server-audience", "not-the-cert", 300);
    assert!(auth
        .validate_access_token(&wrong_fingerprint, &fingerprint)
        .is_err());

    let missing_binding = key.mint(json!({
        "iss": ISSUER,
        "sub": "12345678-1234-5678-9abc-def012345678",
        "username": "TestPlayer",
        "aud": "server-audience",
        "exp": now() + 300,
    }));
    assert!(auth
        .validate_access_token(&missing_binding, &fingerprint)
        .is_err());
}

#[test]
fn jwks_parser_only_accepts_ed25519_keys() {
    let key = TestKey::generate();
    let mut jwks = key.jwks();
    jwks["keys"].as_array_mut().expect("keys").push(json!({
        "kty": "RSA",
        "n": "abc",
        "e": "AQAB",
    }));
    let keys = keys_from_jwks(&jwks);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].kid.as_deref(), Some("test-key"));
}

#[test]
fn live_auth_exposes_the_java_server_audience_and_tokens() {
    let key = TestKey::generate();
    let auth = live_auth(&key, "http://127.0.0.1:9", "fixed-audience");
    assert_eq!(auth.server_audience(), "fixed-audience");
    assert_eq!(auth.session_token().as_deref(), Some("session-token"));
    assert_eq!(auth.identity_token().as_deref(), Some("server-identity"));
}

#[test]
fn grant_and_token_requests_use_the_server_session_bearer() {
    let key = TestKey::generate();
    let grant = "grant-token-123";
    let access = "server-access-456";
    let service = MockService::start(HashMap::from([
        (
            "/server-join/auth-grant".to_string(),
            json!({"authorizationGrant": grant}).to_string(),
        ),
        (
            "/server-join/auth-token".to_string(),
            json!({"accessToken": access}).to_string(),
        ),
    ]));
    let auth = live_auth(&key, &service.base_url(), "server-audience");
    auth.set_certificate_fingerprint("server-cert-fingerprint");

    let identity = key.identity("hytale:client", 300);
    let received_grant = auth
        .request_player_auth_grant(&identity)
        .expect("auth grant");
    assert_eq!(received_grant, grant);

    let received_access = auth
        .exchange_player_grant(&received_grant)
        .expect("auth token");
    assert_eq!(received_access, access);

    let requests = service.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/server-join/auth-grant");
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer session-token")
    );
    let grant_body: Value = serde_json::from_str(&requests[0].body).expect("grant body");
    assert_eq!(grant_body["identityToken"], identity);
    assert_eq!(grant_body["aud"], "server-audience");

    assert_eq!(requests[1].method, "POST");
    assert_eq!(requests[1].path, "/server-join/auth-token");
    assert_eq!(
        requests[1].authorization.as_deref(),
        Some("Bearer session-token")
    );
    let token_body: Value = serde_json::from_str(&requests[1].body).expect("token body");
    assert_eq!(token_body["authorizationGrant"], grant);
    assert_eq!(token_body["x509Fingerprint"], "server-cert-fingerprint");
}

#[test]
fn a_cached_game_session_is_refreshed_without_the_device_flow() {
    let key = TestKey::generate();
    let service = MockService::start(HashMap::from([
        (
            "/game-session/refresh".to_string(),
            json!({
                "sessionToken": "refreshed-session",
                "identityToken": "refreshed-identity",
                "expiresAt": "2099-01-01T00:00:00Z",
            })
            .to_string(),
        ),
        ("/.well-known/jwks.json".to_string(), key.jwks().to_string()),
    ]));
    let cache = std::env::temp_dir().join(format!(
        "hytale-auth-cache-{}-{}.json",
        std::process::id(),
        now()
    ));
    std::fs::write(
        &cache,
        json!({
            "sessionToken": "cached-session",
            "identityToken": "cached-identity",
        })
        .to_string(),
    )
    .expect("cache file");
    let mut config = LiveAuthConfig::loopback(service.base_url());
    config.issuer = ISSUER.to_string();
    config.cache_path = Some(cache.clone());

    let auth = LiveAuth::load_or_connect(config).expect("cached auth");
    assert_eq!(auth.session_token().as_deref(), Some("refreshed-session"));
    assert_eq!(auth.identity_token().as_deref(), Some("refreshed-identity"));
    let requests = service.requests();
    assert_eq!(requests[0].path, "/game-session/refresh");
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer cached-session")
    );
    let _ = std::fs::remove_file(cache);
}

#[test]
fn device_flow_and_game_session_requests_match_the_java_contract() {
    let service = MockService::start(HashMap::from([
        (
            "/oauth2/device/auth".to_string(),
            json!({
                "device_code": "device-code",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://verify.test",
                "verification_uri_complete": "https://verify.test?code=ABCD-EFGH",
                "expires_in": 600,
                "interval": 5,
            })
            .to_string(),
        ),
        (
            "/oauth2/token".to_string(),
            json!({
                "access_token": "oauth-access",
                "refresh_token": "oauth-refresh",
                "expires_in": 3600,
            })
            .to_string(),
        ),
        (
            "/my-account/get-profiles".to_string(),
            json!({"profiles": [
                {"uuid": "12345678-1234-5678-9abc-def012345678", "username": "TestPlayer"}
            ]})
            .to_string(),
        ),
        (
            "/game-session/new".to_string(),
            json!({
                "sessionToken": "session-token",
                "identityToken": "server-identity",
                "expiresAt": "2099-01-01T00:00:00Z",
            })
            .to_string(),
        ),
    ]));
    let client = SessionService::new(LiveAuthConfig::loopback(service.base_url())).expect("client");

    let device = client.request_device_authorization().expect("device auth");
    assert_eq!(device.user_code, "ABCD-EFGH");
    assert_eq!(device.device_code, "device-code");
    assert_eq!(
        device.verification_uri_complete.as_deref(),
        Some("https://verify.test?code=ABCD-EFGH")
    );

    let tokens = client
        .poll_device_token(&device.device_code)
        .expect("device token");
    assert_eq!(tokens.access_token.as_deref(), Some("oauth-access"));
    assert_eq!(tokens.refresh_token.as_deref(), Some("oauth-refresh"));

    let profiles = client.game_profiles("oauth-access").expect("profiles");
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].username, "TestPlayer");

    let session = client
        .create_game_session("oauth-access", &profiles[0].uuid)
        .expect("game session");
    assert_eq!(session.session_token.as_deref(), Some("session-token"));
    assert_eq!(session.identity_token.as_deref(), Some("server-identity"));

    let requests = service.requests();
    assert_eq!(requests[0].path, "/oauth2/device/auth");
    assert!(requests[0].body.contains("client_id=hytale-server"));
    assert!(requests[0].body.contains("openid"));
    assert!(requests[0].body.contains("offline"));
    assert!(
        requests[0].body.contains("auth%3Aserver") || requests[0].body.contains("auth:server"),
        "missing auth:server scope: {}",
        requests[0].body
    );
    assert_eq!(requests[1].path, "/oauth2/token");
    assert!(requests[1]
        .body
        .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"));
    let profile_request = requests
        .iter()
        .find(|request| request.path == "/my-account/get-profiles")
        .expect("profile request");
    assert_eq!(
        profile_request.authorization.as_deref(),
        Some("Bearer oauth-access")
    );
    let session_request = requests
        .iter()
        .find(|request| request.path == "/game-session/new")
        .expect("session request");
    assert_eq!(
        session_request.authorization.as_deref(),
        Some("Bearer oauth-access")
    );
    let body: Value = serde_json::from_str(&session_request.body).expect("session body");
    assert_eq!(body["uuid"], "12345678-1234-5678-9abc-def012345678");
}

#[derive(Clone, Debug)]
struct RecordedRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    body: String,
}

struct MockService {
    address: std::net::SocketAddr,
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
                        std::thread::sleep(std::time::Duration::from_millis(5));
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
        self.requests.lock().expect("requests").clone()
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
    let mut lines = headers.lines();
    let request_line = lines.next()?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next()?.to_string();
    let path = request_parts.next()?.to_string();
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
            method,
            path,
            authorization,
            body,
        },
        response,
    ))
}
