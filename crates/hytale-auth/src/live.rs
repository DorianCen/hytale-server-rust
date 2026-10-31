//! Live Hytale session-service authentication.
//!
//! This is the authority a release client requires (Java
//! `ServerAuthManager`, `SessionServiceClient`, `JWTValidator` and
//! `HandshakeHandler`):
//!
//! 1. the server logs in through the OAuth device flow and opens a game
//!    session (`/game-session/new`), which yields a `sessionToken` bearer and
//!    the server's `identityToken`;
//! 2. a joining client sends its identity token in `Connect`;
//! 3. the server validates it against the live JWKS, asks
//!    `/server-join/auth-grant` for a grant and sends `AuthGrant`;
//! 4. the client answers with `AuthToken`; the server validates the access
//!    token (including the `cnf.x5t#S256` mTLS binding), exchanges the grant at
//!    `/server-join/auth-token` and sends `ServerAuthToken`.
//!
//! The local `AuthService` remains the offline/dev authority. This module is
//! the production/live one.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde_json::Value;

use crate::{is_uuid, AuthError, Result, SCOPE_CLIENT, SCOPE_EDITOR};

/// Live session service base URL (Java `AuthConfig.SESSION_SERVICE_URL`).
pub const DEFAULT_SESSION_SERVICE_URL: &str = "https://sessions.hytale.com";
/// Live account-data base URL (Java `AuthConfig.ACCOUNT_DATA_URL`).
pub const DEFAULT_ACCOUNT_DATA_URL: &str = "https://account-data.hytale.com";
/// OAuth device authorization endpoint.
pub const DEFAULT_OAUTH_DEVICE_URL: &str = "https://oauth.accounts.hytale.com/oauth2/device/auth";
/// OAuth token endpoint.
pub const DEFAULT_OAUTH_TOKEN_URL: &str = "https://oauth.accounts.hytale.com/oauth2/token";
/// Java `AuthConfig.CLIENT_ID`.
pub const DEFAULT_CLIENT_ID: &str = "hytale-server";
/// Java `AuthConfig.SCOPES`.
pub const DEFAULT_SCOPES: &[&str] = &["openid", "offline", "auth:server"];
/// Java `JWTValidator.CLOCK_SKEW_SECONDS`.
pub const CLOCK_SKEW_SECONDS: i64 = 60;
/// Default device-flow poll interval (Java `DEVICE_POLL_INTERVAL_SECONDS`).
pub const DEFAULT_DEVICE_POLL_INTERVAL_SECONDS: u64 = 15;

/// Endpoints and policy for the live authority. Overridable so tests can
/// point at a loopback session-service stub.
#[derive(Clone, Debug)]
pub struct LiveAuthConfig {
    /// Session service base URL (also the expected JWT issuer).
    pub session_service_url: String,
    /// Account-data base URL.
    pub account_data_url: String,
    /// OAuth device authorization URL.
    pub oauth_device_url: String,
    /// OAuth token URL.
    pub oauth_token_url: String,
    /// OAuth client id.
    pub client_id: String,
    /// OAuth scopes.
    pub scopes: Vec<String>,
    /// HTTP `User-Agent`.
    pub user_agent: String,
    /// Expected JWT `iss`; defaults to `session_service_url`.
    pub issuer: String,
    /// Fixed server audience (Java `HYTALE_SERVER_AUDIENCE`). When absent a
    /// random UUID is generated per process, matching the Java server.
    pub audience: Option<String>,
    /// HTTP request timeout.
    pub timeout: Duration,
    /// Device-flow poll interval.
    pub device_poll_interval: Duration,
}

impl Default for LiveAuthConfig {
    fn default() -> Self {
        Self {
            session_service_url: DEFAULT_SESSION_SERVICE_URL.to_string(),
            account_data_url: DEFAULT_ACCOUNT_DATA_URL.to_string(),
            oauth_device_url: DEFAULT_OAUTH_DEVICE_URL.to_string(),
            oauth_token_url: DEFAULT_OAUTH_TOKEN_URL.to_string(),
            client_id: DEFAULT_CLIENT_ID.to_string(),
            scopes: DEFAULT_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect(),
            user_agent: format!("HytaleServer/{}", env!("CARGO_PKG_VERSION")),
            issuer: DEFAULT_SESSION_SERVICE_URL.to_string(),
            audience: std::env::var("HYTALE_SERVER_AUDIENCE").ok(),
            timeout: Duration::from_secs(30),
            device_poll_interval: Duration::from_secs(DEFAULT_DEVICE_POLL_INTERVAL_SECONDS),
        }
    }
}

impl LiveAuthConfig {
    /// Uses a loopback stub for every service endpoint (tests).
    pub fn loopback(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let base_url = base_url.trim_end_matches('/').to_string();
        Self {
            session_service_url: base_url.clone(),
            account_data_url: base_url.clone(),
            oauth_device_url: format!("{base_url}/oauth2/device/auth"),
            oauth_token_url: format!("{base_url}/oauth2/token"),
            issuer: base_url,
            audience: None,
            ..Self::default()
        }
    }
}

/// OAuth device-flow authorization returned by `oauth2/device/auth`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceAuthorization {
    /// Device code used for polling.
    pub device_code: String,
    /// Human-readable code.
    pub user_code: String,
    /// Verification page.
    pub verification_uri: String,
    /// Verification page with the code pre-filled.
    pub verification_uri_complete: Option<String>,
    /// Lifetime in seconds.
    pub expires_in: u64,
    /// Poll interval in seconds.
    pub interval: u64,
}

/// One account game profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameProfile {
    /// Profile UUID.
    pub uuid: String,
    /// Profile username.
    pub username: String,
}

/// OAuth token response (subset used by the device flow).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenResponse {
    /// Access token.
    pub access_token: Option<String>,
    /// Refresh token.
    pub refresh_token: Option<String>,
    /// RFC 8628 error code (`authorization_pending`, `slow_down`, ...).
    pub error: Option<String>,
}

/// `/game-session/new` and `/game-session/refresh` response.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameSession {
    /// Bearer for the server-join endpoints.
    pub session_token: Option<String>,
    /// Server identity token sent in `AuthGrant`.
    pub identity_token: Option<String>,
    /// Expiry timestamp (ISO-8601, service supplied).
    pub expires_at: Option<String>,
}

/// An Ed25519 verification key from the session-service JWKS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JwkKey {
    /// Key id.
    pub kid: Option<String>,
    /// Raw public key.
    pub public: [u8; 32],
}

/// Validated live JWT claims.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveClaims {
    /// `sub` — player UUID.
    pub subject: String,
    /// `profile.username` (or top-level `username` in test tokens).
    pub username: String,
    /// `profile.skin` JSON, when present.
    pub skin: Option<String>,
    /// Space-separated scopes.
    pub scope: Option<String>,
    /// `iss`.
    pub issuer: String,
    /// `aud` (first value for arrays).
    pub audience: Option<String>,
    /// `exp`.
    pub expires_at: i64,
    /// `nbf`.
    pub not_before: Option<i64>,
    /// `cnf.x5t#S256`.
    pub certificate_fingerprint: Option<String>,
}

impl LiveClaims {
    /// Whether the scope claim contains `scope`.
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scope
            .as_deref()
            .unwrap_or_default()
            .split_ascii_whitespace()
            .any(|value| value == scope)
    }
}

/// Blocking HTTP client for the OAuth/session services.
pub struct SessionService {
    config: LiveAuthConfig,
    http: ureq::Agent,
}

impl SessionService {
    /// Builds the client.
    pub fn new(config: LiveAuthConfig) -> Result<Self> {
        let http = ureq::Agent::config_builder()
            .timeout_global(Some(config.timeout))
            .user_agent(config.user_agent.clone())
            .http_status_as_error(false)
            .build()
            .new_agent();
        Ok(Self { config, http })
    }

    /// The configuration this client was built from.
    pub fn config(&self) -> &LiveAuthConfig {
        &self.config
    }

    /// Java `requestDeviceAuthorization`: `POST oauth2/device/auth`.
    pub fn request_device_authorization(&self) -> Result<DeviceAuthorization> {
        let scopes = self.config.scopes.join(" ");
        let response = self
            .http
            .post(&self.config.oauth_device_url)
            .send_form([
                ("client_id", self.config.client_id.as_str()),
                ("scope", scopes.as_str()),
            ])
            .map_err(service_error)?;
        let value: Value = success_json(response, "device authorization")?;
        Ok(DeviceAuthorization {
            device_code: string_field(&value, "device_code")?,
            user_code: string_field(&value, "user_code")?,
            verification_uri: string_field(&value, "verification_uri")?,
            verification_uri_complete: value
                .get("verification_uri_complete")
                .and_then(Value::as_str)
                .map(str::to_owned),
            expires_in: value
                .get("expires_in")
                .and_then(Value::as_u64)
                .unwrap_or(600),
            interval: value.get("interval").and_then(Value::as_u64).unwrap_or(5),
        })
    }

    /// Java `pollDeviceToken`: `POST oauth2/token` with the device grant.
    ///
    /// A 400 response is part of the protocol and carries `error`; it is
    /// returned as a normal [`TokenResponse`].
    pub fn poll_device_token(&self, device_code: &str) -> Result<TokenResponse> {
        let response = self
            .http
            .post(&self.config.oauth_token_url)
            .send_form([
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", self.config.client_id.as_str()),
                ("device_code", device_code),
            ])
            .map_err(service_error)?;
        parse_token_response(response)
    }

    /// Java `refreshTokens`: `grant_type=refresh_token`.
    pub fn refresh_tokens(&self, refresh_token: &str) -> Result<TokenResponse> {
        let response = self
            .http
            .post(&self.config.oauth_token_url)
            .send_form([
                ("grant_type", "refresh_token"),
                ("client_id", self.config.client_id.as_str()),
                ("refresh_token", refresh_token),
            ])
            .map_err(service_error)?;
        parse_token_response(response)
    }

    /// Java `getGameProfiles`: `GET /my-account/get-profiles`.
    pub fn game_profiles(&self, oauth_access_token: &str) -> Result<Vec<GameProfile>> {
        let response = self
            .http
            .get(format!(
                "{}/my-account/get-profiles",
                self.config.account_data_url
            ))
            .header("Authorization", format!("Bearer {oauth_access_token}"))
            .call()
            .map_err(service_error)?;
        let value: Value = success_json(response, "account profiles")?;
        let profiles = value
            .get("profiles")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(profiles
            .iter()
            .filter_map(|profile| {
                Some(GameProfile {
                    uuid: profile.get("uuid")?.as_str()?.to_owned(),
                    username: profile
                        .get("username")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned(),
                })
            })
            .collect())
    }

    /// Java `createGameSession`: `POST /game-session/new`.
    pub fn create_game_session(
        &self,
        oauth_access_token: &str,
        profile_uuid: &str,
    ) -> Result<GameSession> {
        let response = self
            .http
            .post(format!(
                "{}/game-session/new",
                self.config.session_service_url
            ))
            .header("Authorization", format!("Bearer {oauth_access_token}"))
            .send_json(serde_json::json!({ "uuid": profile_uuid }))
            .map_err(service_error)?;
        let value: Value = success_json(response, "game-session/new")?;
        Ok(game_session_from_value(&value))
    }

    /// Java `refreshSessionAsync`: `POST /game-session/refresh`.
    pub fn refresh_game_session(&self, session_token: &str) -> Result<GameSession> {
        let response = self
            .http
            .post(format!(
                "{}/game-session/refresh",
                self.config.session_service_url
            ))
            .header("Authorization", format!("Bearer {session_token}"))
            .send_empty()
            .map_err(service_error)?;
        let value: Value = success_json(response, "game-session/refresh")?;
        Ok(game_session_from_value(&value))
    }

    /// Java `getJwks`: `GET /.well-known/jwks.json`.
    pub fn jwks(&self) -> Result<Value> {
        let response = self
            .http
            .get(format!(
                "{}/.well-known/jwks.json",
                self.config.session_service_url
            ))
            .call()
            .map_err(service_error)?;
        success_json(response, "session-service JWKS")
    }

    /// Java `requestAuthorizationGrantAsync`: `POST /server-join/auth-grant`.
    pub fn request_authorization_grant(
        &self,
        identity_token: &str,
        server_audience: &str,
        bearer_token: &str,
    ) -> Result<String> {
        let response = self
            .http
            .post(format!(
                "{}/server-join/auth-grant",
                self.config.session_service_url
            ))
            .header("Authorization", format!("Bearer {bearer_token}"))
            .send_json(serde_json::json!({
                "identityToken": identity_token,
                "aud": server_audience,
            }))
            .map_err(service_error)?;
        let value: Value = success_json(response, "server-join/auth-grant")?;
        string_field(&value, "authorizationGrant")
    }

    /// Java `exchangeAuthGrantForTokenAsync`: `POST /server-join/auth-token`.
    pub fn exchange_auth_grant_for_token(
        &self,
        authorization_grant: &str,
        x509_fingerprint: &str,
        bearer_token: &str,
    ) -> Result<String> {
        let response = self
            .http
            .post(format!(
                "{}/server-join/auth-token",
                self.config.session_service_url
            ))
            .header("Authorization", format!("Bearer {bearer_token}"))
            .send_json(serde_json::json!({
                "authorizationGrant": authorization_grant,
                "x509Fingerprint": x509_fingerprint,
            }))
            .map_err(service_error)?;
        let value: Value = success_json(response, "server-join/auth-token")?;
        string_field(&value, "accessToken")
    }
}

/// The live authority: service client plus the server's game-session
/// credentials and cached JWKS.
pub struct LiveAuth {
    service: SessionService,
    server_audience: String,
    state: Mutex<GameSession>,
    jwks: Mutex<Vec<JwkKey>>,
    certificate_fingerprint: Mutex<Option<String>>,
}

impl std::fmt::Debug for LiveAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LiveAuth")
            .field("server_audience", &self.server_audience)
            .field("has_session_token", &self.session_token().is_some())
            .field("has_identity_token", &self.identity_token().is_some())
            .field(
                "jwks_keys",
                &self.jwks.lock().map(|keys| keys.len()).unwrap_or(0),
            )
            .finish_non_exhaustive()
    }
}

impl LiveAuth {
    /// Authenticates with the OAuth device flow, selects the account's first
    /// game profile and opens a server game session.
    ///
    /// Blocking: call from `main` or `spawn_blocking`, not on the reactor.
    pub fn from_device_flow(config: LiveAuthConfig) -> Result<Arc<Self>> {
        let service = SessionService::new(config)?;
        let device = service.request_device_authorization()?;
        let target = device
            .verification_uri_complete
            .as_deref()
            .unwrap_or(device.verification_uri.as_str());
        println!(
            "To authenticate this server, open {target} and enter code {}.",
            device.user_code
        );
        println!("Waiting for Hytale authorization...");

        let interval = service
            .config()
            .device_poll_interval
            .max(Duration::from_secs(device.interval.max(1)));
        let deadline = Instant::now() + Duration::from_secs(device.expires_in.clamp(60, 1800));
        let access_token = loop {
            if Instant::now() >= deadline {
                return Err(AuthError::Service(
                    "device authorization expired".to_string(),
                ));
            }
            std::thread::sleep(interval);
            let tokens = service.poll_device_token(&device.device_code)?;
            match tokens.error.as_deref() {
                Some("authorization_pending") => continue,
                Some("slow_down") => {
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
                Some(error) => {
                    return Err(AuthError::Service(format!(
                        "device authorization failed: {error}"
                    )))
                }
                None => {}
            }
            if let Some(token) = tokens.access_token {
                break token;
            }
        };

        let profiles = service.game_profiles(&access_token)?;
        let profile = profiles.first().ok_or_else(|| {
            AuthError::Service("the account has no Hytale game profiles".to_string())
        })?;
        println!(
            "Authenticated as {} ({}). Opening a game session...",
            profile.username, profile.uuid
        );
        let session = service.create_game_session(&access_token, &profile.uuid)?;
        if session.session_token.is_none() || session.identity_token.is_none() {
            return Err(AuthError::Service(
                "game-session/new returned incomplete credentials".to_string(),
            ));
        }
        Self::assemble(service, session)
    }

    /// Uses externally supplied session/identity tokens (Java
    /// `HYTALE_SERVER_SESSION_TOKEN` / `HYTALE_SERVER_IDENTITY_TOKEN`).
    pub fn from_tokens(
        config: LiveAuthConfig,
        session_token: impl Into<String>,
        identity_token: impl Into<String>,
    ) -> Result<Arc<Self>> {
        let service = SessionService::new(config)?;
        let session = GameSession {
            session_token: Some(session_token.into()),
            identity_token: Some(identity_token.into()),
            expires_at: None,
        };
        Self::assemble(service, session)
    }

    /// Like [`Self::from_tokens`] but installs an explicit JWKS document
    /// instead of fetching one. Intended for tests and offline mirrors.
    pub fn with_jwks(
        config: LiveAuthConfig,
        session_token: impl Into<String>,
        identity_token: impl Into<String>,
        jwks: &Value,
    ) -> Result<Arc<Self>> {
        let service = SessionService::new(config)?;
        let keys = keys_from_jwks(jwks);
        if keys.is_empty() {
            return Err(AuthError::Service(
                "JWKS contains no Ed25519 keys".to_string(),
            ));
        }
        let session = GameSession {
            session_token: Some(session_token.into()),
            identity_token: Some(identity_token.into()),
            expires_at: None,
        };
        let audience = audience_for(service.config());
        Ok(Arc::new(Self {
            service,
            server_audience: audience,
            state: Mutex::new(session),
            jwks: Mutex::new(keys),
            certificate_fingerprint: Mutex::new(None),
        }))
    }

    fn assemble(service: SessionService, session: GameSession) -> Result<Arc<Self>> {
        let audience = audience_for(service.config());
        let auth = Arc::new(Self {
            service,
            server_audience: audience,
            state: Mutex::new(session),
            jwks: Mutex::new(Vec::new()),
            certificate_fingerprint: Mutex::new(None),
        });
        auth.refresh_jwks()?;
        Ok(auth)
    }

    /// The session-service client.
    pub fn service(&self) -> &SessionService {
        &self.service
    }

    /// Java `AuthConfig.getServerAudience`.
    pub fn server_audience(&self) -> String {
        self.server_audience.clone()
    }

    /// Sets the SHA-256 base64url fingerprint of the server's TLS certificate.
    pub fn set_certificate_fingerprint(&self, fingerprint: impl Into<String>) {
        *self
            .certificate_fingerprint
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(fingerprint.into());
    }

    /// The server certificate fingerprint used for the grant exchange.
    pub fn certificate_fingerprint(&self) -> Option<String> {
        self.certificate_fingerprint
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Java `getSessionToken`.
    pub fn session_token(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .session_token
            .clone()
    }

    /// Java `getIdentityToken`.
    pub fn identity_token(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .identity_token
            .clone()
    }

    /// Refreshes the cached session-service JWKS.
    pub fn refresh_jwks(&self) -> Result<()> {
        let jwks = self.service.jwks()?;
        let keys = keys_from_jwks(&jwks);
        if keys.is_empty() {
            return Err(AuthError::Service(
                "session-service JWKS contains no Ed25519 keys".to_string(),
            ));
        }
        *self
            .jwks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = keys;
        Ok(())
    }

    /// Current verification keys.
    pub fn verification_keys(&self) -> Vec<JwkKey> {
        self.jwks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Java `refreshSessionAsync`.
    pub fn refresh(&self) -> Result<()> {
        let session_token = self
            .session_token()
            .ok_or_else(|| AuthError::Service("server has no game session".to_string()))?;
        let refreshed = self.service.refresh_game_session(&session_token)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(session_token) = refreshed.session_token {
            state.session_token = Some(session_token);
        }
        if refreshed.identity_token.is_some() {
            state.identity_token = refreshed.identity_token;
        }
        state.expires_at = refreshed.expires_at;
        Ok(())
    }

    /// Validates a client identity token (`Connect.identityToken`).
    ///
    /// Java `JWTValidator.validateIdentityToken`: signature, issuer, expiry,
    /// required scope, subject UUID and profile username. Identity tokens are
    /// deliberately not certificate-bound and have no audience check.
    pub fn validate_identity_token(&self, token: &str, required_scope: &str) -> Result<LiveClaims> {
        let claims = self.verify_token(token)?;
        self.check_standard_claims(&claims)?;
        if claims.subject.is_empty() || !is_uuid(&claims.subject) {
            return Err(AuthError::InvalidSubject(claims.subject));
        }
        if claims.username.is_empty() {
            return Err(AuthError::InvalidClaim {
                claim: "profile.username",
                message: "username is missing".to_string(),
            });
        }
        if !claims.has_scope(required_scope) {
            return Err(AuthError::InvalidClaim {
                claim: "scope",
                message: format!("required scope `{required_scope}` is missing"),
            });
        }
        Ok(claims)
    }

    /// Validates a client access token (`AuthToken.accessToken`).
    ///
    /// Java `JWTValidator.validateToken`: audience is the server's session id
    /// and `cnf.x5t#S256` must equal the mTLS client certificate fingerprint.
    pub fn validate_access_token(
        &self,
        token: &str,
        client_certificate_fingerprint: &str,
    ) -> Result<LiveClaims> {
        let claims = self.verify_token(token)?;
        self.check_standard_claims(&claims)?;
        if claims.subject.is_empty() || !is_uuid(&claims.subject) {
            return Err(AuthError::InvalidSubject(claims.subject));
        }
        if claims.audience.as_deref() != Some(self.server_audience.as_str()) {
            return Err(AuthError::InvalidClaim {
                claim: "aud",
                message: format!(
                    "expected `{}`, got `{}`",
                    self.server_audience,
                    claims.audience.as_deref().unwrap_or("")
                ),
            });
        }
        let bound = claims.certificate_fingerprint.as_deref().unwrap_or("");
        if bound.is_empty() || !fingerprints_equal(bound, client_certificate_fingerprint) {
            return Err(AuthError::CertificateBinding);
        }
        Ok(claims)
    }

    /// Java `requestAuthorizationGrantAsync` for a joining player.
    pub fn request_player_auth_grant(&self, identity_token: &str) -> Result<String> {
        let bearer = self
            .session_token()
            .ok_or_else(|| AuthError::Service("server has no game session".to_string()))?;
        self.service
            .request_authorization_grant(identity_token, &self.server_audience, &bearer)
    }

    /// Java `exchangeAuthGrantForTokenAsync` for the client-provided grant.
    pub fn exchange_player_grant(&self, server_auth_grant: &str) -> Result<String> {
        let bearer = self
            .session_token()
            .ok_or_else(|| AuthError::Service("server has no game session".to_string()))?;
        let fingerprint = self.certificate_fingerprint().ok_or_else(|| {
            AuthError::Service("server certificate fingerprint is unavailable".to_string())
        })?;
        self.service
            .exchange_auth_grant_for_token(server_auth_grant, &fingerprint, &bearer)
    }

    fn check_standard_claims(&self, claims: &LiveClaims) -> Result<()> {
        if claims.issuer != self.service.config().issuer {
            return Err(AuthError::InvalidClaim {
                claim: "iss",
                message: format!(
                    "expected `{}`, got `{}`",
                    self.service.config().issuer,
                    claims.issuer
                ),
            });
        }
        let now = now_secs();
        if now >= claims.expires_at + CLOCK_SKEW_SECONDS {
            return Err(AuthError::Time(format!(
                "token expired at {} (now {now})",
                claims.expires_at
            )));
        }
        if let Some(not_before) = claims.not_before {
            if now + CLOCK_SKEW_SECONDS < not_before {
                return Err(AuthError::Time(format!(
                    "token is not valid before {not_before} (now {now})"
                )));
            }
        }
        Ok(())
    }

    fn verify_token(&self, token: &str) -> Result<LiveClaims> {
        let parsed = ParsedJwt::parse(token)?;
        if parsed.algorithm != "EdDSA" {
            return Err(AuthError::UnsupportedAlgorithm(parsed.algorithm));
        }
        if !self.verify_signature(&parsed) {
            // Key rotation: refresh once and retry, like Java
            // `verifySignatureWithRetry`.
            let _ = self.refresh_jwks();
            if !self.verify_signature(&parsed) {
                return Err(AuthError::BadSignature);
            }
        }
        Ok(parsed.claims)
    }

    fn verify_signature(&self, parsed: &ParsedJwt) -> bool {
        let keys = self.verification_keys();
        keys.iter().any(|key| {
            let public = UnparsedPublicKey::new(&ED25519, key.public);
            public
                .verify(parsed.signing_input.as_bytes(), &parsed.signature)
                .is_ok()
        })
    }
}

fn audience_for(config: &LiveAuthConfig) -> String {
    config.audience.clone().unwrap_or_else(random_uuid)
}

fn fingerprints_equal(left: &str, right: &str) -> bool {
    let trim = |value: &str| value.trim_end_matches('=').to_string();
    trim(left) == trim(right)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn random_uuid() -> String {
    let mut bytes = [0u8; 16];
    let _ = SystemRandom::new().fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

fn service_error(error: ureq::Error) -> AuthError {
    AuthError::Service(error.to_string())
}

fn success_json(response: ureq::http::Response<ureq::Body>, what: &str) -> Result<Value> {
    let status = response.status();
    let mut body = response.into_body();
    if !status.is_success() {
        let text = body.read_to_string().unwrap_or_default();
        return Err(AuthError::Service(format!(
            "{what} returned HTTP {status}: {}",
            truncate(&text, 512)
        )));
    }
    body.read_json()
        .map_err(|error| AuthError::Service(format!("{what} response: {error}")))
}

fn parse_token_response(response: ureq::http::Response<ureq::Body>) -> Result<TokenResponse> {
    let status = response.status();
    let mut body = response.into_body();
    let value: Value = body.read_json().map_err(|error| {
        AuthError::Service(format!("oauth token response (HTTP {status}): {error}"))
    })?;
    let error = value
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if !status.is_success() && error.is_none() {
        return Err(AuthError::Service(format!(
            "oauth token request returned HTTP {status}"
        )));
    }
    if status.is_success() && access_token.is_none() && error.is_none() {
        return Err(AuthError::Service(
            "oauth token response has neither access_token nor error".to_string(),
        ));
    }
    Ok(TokenResponse {
        access_token,
        refresh_token: value
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_owned),
        error,
    })
}

fn string_field(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AuthError::Service(format!("response is missing string field `{field}`")))
}

fn game_session_from_value(value: &Value) -> GameSession {
    GameSession {
        session_token: value
            .get("sessionToken")
            .and_then(Value::as_str)
            .map(str::to_owned),
        identity_token: value
            .get("identityToken")
            .and_then(Value::as_str)
            .map(str::to_owned),
        expires_at: value
            .get("expiresAt")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

fn truncate(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((index, _)) => &text[..index],
        None => text,
    }
}

struct ParsedJwt {
    algorithm: String,
    signing_input: String,
    signature: Vec<u8>,
    claims: LiveClaims,
}

impl ParsedJwt {
    fn parse(token: &str) -> Result<Self> {
        let mut segments = token.split('.');
        let header = segments.next().ok_or(AuthError::Malformed)?;
        let payload = segments.next().ok_or(AuthError::Malformed)?;
        let signature = segments.next().ok_or(AuthError::Malformed)?;
        if segments.next().is_some() || signature.is_empty() {
            return Err(AuthError::Malformed);
        }
        let header_json: Value = serde_json::from_slice(&decode_b64url(header)?)?;
        let claims_json: Value = serde_json::from_slice(&decode_b64url(payload)?)?;
        let signature = decode_b64url(signature)?;
        let algorithm = header_json
            .get("alg")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let claims = live_claims(&claims_json)?;
        Ok(Self {
            algorithm,
            signing_input: format!("{header}.{payload}"),
            signature,
            claims,
        })
    }
}

fn live_claims(value: &Value) -> Result<LiveClaims> {
    let subject = value
        .get("sub")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let profile = value.get("profile");
    let username = profile
        .and_then(|profile| profile.get("username"))
        .and_then(Value::as_str)
        .or_else(|| value.get("username").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    let skin = profile
        .and_then(|profile| profile.get("skin"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let scope = value.get("scope").and_then(|scope| match scope {
        Value::String(text) => Some(text.clone()),
        Value::Array(values) => {
            let joined = values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            (!joined.is_empty()).then_some(joined)
        }
        _ => None,
    });
    let audience = value.get("aud").and_then(|aud| match aud {
        Value::String(text) => Some(text.clone()),
        Value::Array(values) => values.first().and_then(Value::as_str).map(str::to_owned),
        _ => None,
    });
    let certificate_fingerprint = value
        .get("cnf")
        .and_then(|confirmation| confirmation.get("x5t#S256"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(LiveClaims {
        subject,
        username,
        skin,
        scope,
        issuer: value
            .get("iss")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        audience,
        expires_at: value.get("exp").and_then(Value::as_i64).ok_or_else(|| {
            AuthError::InvalidClaim {
                claim: "exp",
                message: "expiration is missing".to_string(),
            }
        })?,
        not_before: value.get("nbf").and_then(Value::as_i64),
        certificate_fingerprint,
    })
}

fn decode_b64url(segment: &str) -> Result<Vec<u8>> {
    let segment = segment.trim_end_matches('=');
    URL_SAFE_NO_PAD.decode(segment).map_err(AuthError::Base64)
}

/// Extracts Ed25519 keys from a session-service JWKS document.
pub fn keys_from_jwks(jwks: &Value) -> Vec<JwkKey> {
    let keys = jwks.get("keys").and_then(Value::as_array);
    let Some(keys) = keys else {
        return Vec::new();
    };
    keys.iter()
        .filter(|key| {
            key.get("kty").and_then(Value::as_str) == Some("OKP")
                && key.get("crv").and_then(Value::as_str) == Some("Ed25519")
        })
        .filter_map(|key| {
            let x = decode_b64url(key.get("x")?.as_str()?).ok()?;
            let public = <[u8; 32]>::try_from(x.as_slice()).ok()?;
            Some(JwkKey {
                kid: key.get("kid").and_then(Value::as_str).map(str::to_owned),
                public,
            })
        })
        .collect()
}

/// Scope used for a normal game client (Java `AuthConfig.SCOPE_CLIENT`).
pub const REQUIRED_CLIENT_SCOPE: &str = SCOPE_CLIENT;
/// Scope used for an editor client (rejected by the server today).
pub const REQUIRED_EDITOR_SCOPE: &str = SCOPE_EDITOR;
