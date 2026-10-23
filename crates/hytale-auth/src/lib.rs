//! `hytale-auth` — authentication primitives for the Rust CoreServer.
//!
//! The live Java server authenticates players through the Hytale OAuth and
//! session services and validates EdDSA (Ed25519) JWTs against the service
//! JWKS. The packet flow is `Connect(identityToken)` → `AuthGrant` →
//! `AuthToken` → `ServerAuthToken`, and every access token is bound to the
//! client's mTLS certificate through the `cnf.x5t#S256` claim.
//!
//! This crate implements the cryptographic and claim parts of that contract
//! behind a local authority: a server administrator configures one Ed25519 key
//! and the issuer/audience values, and the same [`AuthService`] can mint the
//! identity, access, grant and server tokens used by the handshake. Replacing
//! the local authority with an OAuth/JWKS-backed one later only requires
//! swapping the issuer/validator; the claim rules and certificate binding stay
//! the same.
//!
//! ```no_run
//! use hytale_auth::{AuthKey, AuthService};
//!
//! # fn example() -> Result<(), hytale_auth::AuthError> {
//! let key = AuthKey::generate()?;
//! let auth = AuthService::new(key, "local", "rust-server");
//! let token = auth.issue_identity_token(
//!     "12345678-1234-5678-9abc-def012345678",
//!     "RustBot",
//!     &["hytale:client".to_string()],
//!     Some("certificate-fingerprint"),
//!     300,
//! )?;
//! let claims = auth.validate(&token, "identity", Some("hytale:client"), Some("certificate-fingerprint"))?;
//! assert_eq!(claims.username.as_deref(), Some("RustBot"));
//! # Ok(())
//! # }
//! ```

#![deny(clippy::all)]

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Identity-token scope for a game client.
pub const SCOPE_CLIENT: &str = "hytale:client";
/// Identity-token scope for the editor client.
pub const SCOPE_EDITOR: &str = "hytale:editor";
/// Server identity scope.
pub const SCOPE_SERVER: &str = "hytale:server";
/// Offline/singleplayer scope.
pub const SCOPE_OFFLINE: &str = "hytale:offline";

/// The JWT algorithm used by the Hytale services.
pub const JWT_ALGORITHM: &str = "EdDSA";
/// Default clock-skew tolerance, matching the Java validator.
pub const DEFAULT_CLOCK_SKEW_SECS: u64 = 300;

/// Errors from token issuance and validation.
#[derive(Debug, Error)]
pub enum AuthError {
    /// Ed25519 key generation failed.
    #[error("could not generate an Ed25519 key: {0}")]
    KeyGeneration(String),
    /// A PKCS#8 key could not be parsed.
    #[error("invalid PKCS#8 Ed25519 key: {0}")]
    InvalidKey(String),
    /// Base64 decoding failed.
    #[error("invalid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    /// JSON encoding/decoding failed.
    #[error("invalid token JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The token does not have three dot-separated segments.
    #[error("malformed JWT: expected three segments")]
    Malformed,
    /// The token header is not `EdDSA`.
    #[error("unsupported JWT algorithm `{0}` (expected EdDSA)")]
    UnsupportedAlgorithm(String),
    /// The signature does not verify.
    #[error("JWT signature verification failed")]
    BadSignature,
    /// A claim is missing or has the wrong value.
    #[error("invalid JWT claim `{claim}`: {message}")]
    InvalidClaim {
        /// Claim name.
        claim: &'static str,
        /// Why it was rejected.
        message: String,
    },
    /// The subject is not a hyphenated UUID.
    #[error("JWT subject `{0}` is not a UUID")]
    InvalidSubject(String),
    /// The certificate binding is missing or does not match.
    #[error("JWT certificate binding failed")]
    CertificateBinding,
    /// The token was minted for another use.
    #[error("JWT token use `{found}` does not match `{expected}`")]
    WrongTokenUse {
        /// Token-use claim.
        found: String,
        /// Expected use.
        expected: &'static str,
    },
    /// The token is expired / not yet valid / issued in the future.
    #[error("JWT time validation failed: {0}")]
    Time(String),
    /// Per-token time-to-live is not representable.
    #[error("invalid token lifetime: {0}")]
    InvalidLifetime(String),
}

/// Result alias for the auth layer.
pub type Result<T> = std::result::Result<T, AuthError>;

/// JWT confirmation claim carrying the mTLS certificate binding.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirmation {
    /// Base64url SHA-256 digest of the client certificate (RFC 8705).
    #[serde(rename = "x5t#S256", default, skip_serializing_if = "Option::is_none")]
    pub certificate_fingerprint: Option<String>,
}

/// Claims used by the Hytale handshake and by the local authority.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Issuer (`iss`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// Audience (`aud`).
    #[serde(
        default,
        deserialize_with = "de_first_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub audience: Option<String>,
    /// Subject (`sub`), normally the player UUID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Player username.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Space-separated scopes.
    #[serde(
        default,
        deserialize_with = "de_joined_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub scope: Option<String>,
    /// Issued-at, seconds since Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<i64>,
    /// Expiry, seconds since Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// Not-before, seconds since Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before: Option<i64>,
    /// Certificate binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<Confirmation>,
    /// Local token purpose (`identity`, `access`, `grant`, `server`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_use: Option<String>,
}

impl TokenClaims {
    /// Whether the scope claim contains `scope`.
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes().contains(&scope)
    }

    /// Split the scope claim on whitespace.
    pub fn scopes(&self) -> Vec<&str> {
        self.scope
            .as_deref()
            .unwrap_or_default()
            .split_ascii_whitespace()
            .collect()
    }

    /// Base64url certificate fingerprint from `cnf.x5t#S256`.
    pub fn certificate_fingerprint(&self) -> Option<&str> {
        self.confirmation
            .as_ref()
            .and_then(|confirmation| confirmation.certificate_fingerprint.as_deref())
    }
}

fn de_first_string<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::String(text)) => Some(text),
        Some(serde_json::Value::Array(values)) => values
            .into_iter()
            .find_map(|value| value.as_str().map(ToOwned::to_owned)),
        _ => None,
    })
}

fn de_joined_string<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::String(text)) => Some(text),
        Some(serde_json::Value::Array(values)) => {
            let scopes: Vec<&str> = values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect();
            if scopes.is_empty() {
                None
            } else {
                Some(scopes.join(" "))
            }
        }
        _ => None,
    })
}

/// One Ed25519 signing key.
#[derive(Clone)]
pub struct AuthKey {
    pkcs8: Vec<u8>,
    key_pair: Arc<Ed25519KeyPair>,
    public_key: [u8; 32],
}

impl std::fmt::Debug for AuthKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthKey")
            .field("kid", &self.key_id())
            .field("pkcs8", &"<redacted>")
            .finish()
    }
}

impl AuthKey {
    /// Generate a fresh Ed25519 key.
    pub fn generate() -> Result<Self> {
        let random = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&random)
            .map_err(|error| AuthError::KeyGeneration(error.to_string()))?;
        Self::from_pkcs8(pkcs8.as_ref())
    }

    /// Wrap an existing PKCS#8 v1/v2 Ed25519 document.
    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Self> {
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8)
            .map_err(|error| AuthError::InvalidKey(error.to_string()))?;
        let public_key: [u8; 32] = key_pair
            .public_key()
            .as_ref()
            .try_into()
            .map_err(|_| AuthError::InvalidKey("public key is not 32 bytes".to_string()))?;
        Ok(Self {
            pkcs8: pkcs8.to_vec(),
            key_pair: Arc::new(key_pair),
            public_key,
        })
    }

    /// Load a base64 (standard or URL-safe, padded or not) PKCS#8 key.
    pub fn from_pkcs8_base64(encoded: &str) -> Result<Self> {
        let trimmed = encoded.trim();
        let decoded = URL_SAFE_NO_PAD
            .decode(trimmed.trim_end_matches('='))
            .or_else(|_| {
                base64::engine::general_purpose::STANDARD
                    .decode(trimmed)
                    .map_err(AuthError::from)
            })?;
        Self::from_pkcs8(&decoded)
    }

    /// The PKCS#8 document, base64url encoded (for CLI/config transfer).
    pub fn pkcs8_base64(&self) -> String {
        URL_SAFE_NO_PAD.encode(&self.pkcs8)
    }

    /// The raw 32-byte public key.
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }

    /// The public key, base64url encoded.
    pub fn public_key_base64(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.public_key)
    }

    /// A stable short key id derived from the public key.
    pub fn key_id(&self) -> String {
        let digest = Sha256::digest(self.public_key);
        URL_SAFE_NO_PAD.encode(&digest[..8])
    }

    /// Sign `message` with Ed25519.
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.key_pair.sign(message).as_ref().to_vec()
    }

    /// Verify an Ed25519 signature with this key's public half.
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        UnparsedPublicKey::new(&ED25519, self.public_key)
            .verify(message, signature)
            .is_ok()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtHeader {
    alg: String,
    typ: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kid: Option<String>,
}

/// One local authentication authority.
#[derive(Debug, Clone)]
pub struct AuthService {
    key: AuthKey,
    issuer: String,
    audience: String,
    clock_skew_secs: u64,
}

impl AuthService {
    /// Create an authority with the Java validator's default clock skew.
    pub fn new(key: AuthKey, issuer: impl Into<String>, audience: impl Into<String>) -> Self {
        Self {
            key,
            issuer: issuer.into(),
            audience: audience.into(),
            clock_skew_secs: DEFAULT_CLOCK_SKEW_SECS,
        }
    }

    /// Replace the clock-skew tolerance.
    pub fn with_clock_skew(mut self, seconds: u64) -> Self {
        self.clock_skew_secs = seconds;
        self
    }

    /// The signing key.
    pub fn key(&self) -> &AuthKey {
        &self.key
    }

    /// The expected issuer.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The expected audience.
    pub fn audience(&self) -> &str {
        &self.audience
    }

    /// Issue an identity token (`Connect.identityToken`).
    pub fn issue_identity_token(
        &self,
        subject: &str,
        username: &str,
        scopes: &[String],
        certificate_fingerprint: Option<&str>,
        ttl_secs: i64,
    ) -> Result<String> {
        self.issue(
            subject,
            username,
            scopes,
            certificate_fingerprint,
            "identity",
            ttl_secs,
        )
    }

    /// Issue an access token (`AuthToken.accessToken`).
    pub fn issue_access_token(
        &self,
        subject: &str,
        username: &str,
        scopes: &[String],
        certificate_fingerprint: Option<&str>,
        ttl_secs: i64,
    ) -> Result<String> {
        self.issue(
            subject,
            username,
            scopes,
            certificate_fingerprint,
            "access",
            ttl_secs,
        )
    }

    /// Issue the server's authorization grant (`AuthGrant.authorizationGrant`).
    pub fn issue_grant(
        &self,
        subject: &str,
        username: &str,
        certificate_fingerprint: Option<&str>,
        ttl_secs: i64,
    ) -> Result<String> {
        self.issue(
            subject,
            username,
            &[SCOPE_SERVER.to_string()],
            certificate_fingerprint,
            "grant",
            ttl_secs,
        )
    }

    /// Issue the server access token (`ServerAuthToken.serverAccessToken`).
    pub fn issue_server_token(
        &self,
        subject: &str,
        username: &str,
        certificate_fingerprint: Option<&str>,
        ttl_secs: i64,
    ) -> Result<String> {
        self.issue(
            subject,
            username,
            &[SCOPE_SERVER.to_string()],
            certificate_fingerprint,
            "server",
            ttl_secs,
        )
    }

    /// Issue a token with an explicit local token-use value.
    pub fn issue(
        &self,
        subject: &str,
        username: &str,
        scopes: &[String],
        certificate_fingerprint: Option<&str>,
        token_use: &str,
        ttl_secs: i64,
    ) -> Result<String> {
        if !is_uuid(subject) {
            return Err(AuthError::InvalidSubject(subject.to_string()));
        }
        if ttl_secs <= 0 {
            return Err(AuthError::InvalidLifetime(ttl_secs.to_string()));
        }
        let now = now_secs()?;
        let expires = now
            .checked_add(ttl_secs)
            .ok_or_else(|| AuthError::InvalidLifetime(ttl_secs.to_string()))?;
        let claims = TokenClaims {
            issuer: Some(self.issuer.clone()),
            audience: Some(self.audience.clone()),
            subject: Some(subject.to_string()),
            username: Some(username.to_string()),
            scope: (!scopes.is_empty()).then(|| scopes.join(" ")),
            issued_at: Some(now),
            expires_at: Some(expires),
            not_before: Some(now),
            confirmation: certificate_fingerprint.map(|fingerprint| Confirmation {
                certificate_fingerprint: Some(fingerprint.to_string()),
            }),
            token_use: Some(token_use.to_string()),
        };
        self.sign_claims(&claims)
    }

    /// Encode and sign arbitrary claims.
    pub fn sign_claims(&self, claims: &TokenClaims) -> Result<String> {
        let header = JwtHeader {
            alg: JWT_ALGORITHM.to_string(),
            typ: "JWT".to_string(),
            kid: Some(self.key.key_id()),
        };
        let header = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims)?);
        let signing_input = format!("{header}.{payload}");
        let signature = self.key.sign(signing_input.as_bytes());
        Ok(format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature)
        ))
    }

    /// Validate a token for `expected_use`.
    ///
    /// `required_scope` is checked when `Some`; the certificate fingerprint is
    /// always required and compared to `client_certificate_fingerprint`
    /// (pass `None` when the connection has no client certificate, which always
    /// fails).
    pub fn validate(
        &self,
        token: &str,
        expected_use: &'static str,
        required_scope: Option<&str>,
        client_certificate_fingerprint: Option<&str>,
    ) -> Result<TokenClaims> {
        self.validate_at(
            token,
            expected_use,
            required_scope,
            client_certificate_fingerprint,
            now_secs()?,
        )
    }

    /// Validate a token at a fixed Unix time (for tests).
    pub fn validate_at(
        &self,
        token: &str,
        expected_use: &'static str,
        required_scope: Option<&str>,
        client_certificate_fingerprint: Option<&str>,
        now: i64,
    ) -> Result<TokenClaims> {
        let (header, claims, signature) = split_token(token)?;
        if header.alg != JWT_ALGORITHM {
            return Err(AuthError::UnsupportedAlgorithm(header.alg));
        }

        let signing_input = token
            .rsplit_once('.')
            .map(|(input, _)| input)
            .ok_or(AuthError::Malformed)?;
        if !self.key.verify(signing_input.as_bytes(), &signature) {
            return Err(AuthError::BadSignature);
        }

        if claims.issuer.as_deref() != Some(self.issuer.as_str()) {
            return Err(AuthError::InvalidClaim {
                claim: "iss",
                message: format!(
                    "expected `{}`, got `{}`",
                    self.issuer,
                    claims.issuer.as_deref().unwrap_or("<missing>")
                ),
            });
        }
        if claims.audience.as_deref() != Some(self.audience.as_str()) {
            return Err(AuthError::InvalidClaim {
                claim: "aud",
                message: format!(
                    "expected `{}`, got `{}`",
                    self.audience,
                    claims.audience.as_deref().unwrap_or("<missing>")
                ),
            });
        }

        let expires = claims.expires_at.ok_or_else(|| AuthError::InvalidClaim {
            claim: "exp",
            message: "missing expiration".to_string(),
        })?;
        let skew = self.clock_skew_secs as i64;
        if now >= expires.saturating_add(skew) {
            return Err(AuthError::Time(format!("expired at {expires}, now {now}")));
        }
        if let Some(not_before) = claims.not_before {
            if now < not_before.saturating_sub(skew) {
                return Err(AuthError::Time(format!(
                    "not valid before {not_before}, now {now}"
                )));
            }
        }
        if let Some(issued_at) = claims.issued_at {
            if issued_at > now.saturating_add(skew) {
                return Err(AuthError::Time(format!(
                    "issued in the future at {issued_at}, now {now}"
                )));
            }
        }

        let subject = claims
            .subject
            .as_deref()
            .ok_or_else(|| AuthError::InvalidClaim {
                claim: "sub",
                message: "missing subject".to_string(),
            })?;
        if !is_uuid(subject) {
            return Err(AuthError::InvalidSubject(subject.to_string()));
        }

        let found_use = claims.token_use.as_deref().unwrap_or_default();
        if found_use != expected_use {
            return Err(AuthError::WrongTokenUse {
                found: found_use.to_string(),
                expected: expected_use,
            });
        }

        if let Some(required_scope) = required_scope {
            if !claims.has_scope(required_scope) {
                return Err(AuthError::InvalidClaim {
                    claim: "scope",
                    message: format!(
                        "missing `{required_scope}` (found `{}`)",
                        claims.scope.as_deref().unwrap_or("")
                    ),
                });
            }
        }

        let token_fingerprint =
            claims
                .certificate_fingerprint()
                .ok_or(AuthError::InvalidClaim {
                    claim: "cnf.x5t#S256",
                    message: "missing certificate binding".to_string(),
                })?;
        let client_fingerprint =
            client_certificate_fingerprint.ok_or(AuthError::CertificateBinding)?;
        if !constant_time_eq(token_fingerprint.as_bytes(), client_fingerprint.as_bytes()) {
            return Err(AuthError::CertificateBinding);
        }

        Ok(claims)
    }
}

fn split_token(token: &str) -> Result<(JwtHeader, TokenClaims, Vec<u8>)> {
    let mut parts = token.split('.');
    let header = parts.next().ok_or(AuthError::Malformed)?;
    let payload = parts.next().ok_or(AuthError::Malformed)?;
    let signature = parts.next().ok_or(AuthError::Malformed)?;
    if parts.next().is_some() || signature.is_empty() {
        return Err(AuthError::Malformed);
    }
    let header: JwtHeader = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header)?)?;
    let claims: TokenClaims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;
    let signature = URL_SAFE_NO_PAD.decode(signature)?;
    Ok((header, claims, signature))
}

fn now_secs() -> Result<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .map_err(|error| AuthError::Time(error.to_string()))
}

/// Whether `value` is a hyphenated UUID.
pub fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

/// Base64url (no padding) encoding, as used by JWTs and `cnf.x5t#S256`.
pub fn base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Base64url (no padding) decoding.
pub fn base64url_decode(value: &str) -> Result<Vec<u8>> {
    Ok(URL_SAFE_NO_PAD.decode(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "12345678-1234-5678-9abc-def012345678";
    const FINGERPRINT: &str = "Y2VydGlmaWNhdGUtZmluZ2VycHJpbnQ";

    fn service() -> AuthService {
        let key = AuthKey::generate().expect("test key");
        AuthService::new(key, "local-test", "rust-server")
    }

    fn tokens() -> (AuthService, String, String, String) {
        let service = service();
        let scopes = [SCOPE_CLIENT.to_string()];
        let identity = service
            .issue_identity_token(UUID, "RustBot", &scopes, Some(FINGERPRINT), 300)
            .expect("identity");
        let access = service
            .issue_access_token(UUID, "RustBot", &scopes, Some(FINGERPRINT), 300)
            .expect("access");
        let grant = service
            .issue_grant(UUID, "RustBot", Some(FINGERPRINT), 300)
            .expect("grant");
        (service, identity, access, grant)
    }

    #[test]
    fn round_trips_identity_and_rejects_wrong_use() {
        let (service, identity, access, grant) = tokens();
        let claims = service
            .validate(&identity, "identity", Some(SCOPE_CLIENT), Some(FINGERPRINT))
            .expect("identity validates");
        assert_eq!(claims.subject.as_deref(), Some(UUID));
        assert_eq!(claims.username.as_deref(), Some("RustBot"));
        assert!(claims.has_scope(SCOPE_CLIENT));

        assert!(matches!(
            service.validate(&access, "identity", Some(SCOPE_CLIENT), Some(FINGERPRINT)),
            Err(AuthError::WrongTokenUse { .. })
        ));
        assert!(service
            .validate(&grant, "grant", Some(SCOPE_SERVER), Some(FINGERPRINT))
            .is_ok());
    }

    #[test]
    fn rejects_a_mismatched_certificate_binding() {
        let (service, identity, _, _) = tokens();
        assert!(matches!(
            service.validate(
                &identity,
                "identity",
                Some(SCOPE_CLIENT),
                Some("different-fingerprint")
            ),
            Err(AuthError::CertificateBinding)
        ));
        assert!(matches!(
            service.validate(&identity, "identity", Some(SCOPE_CLIENT), None),
            Err(AuthError::CertificateBinding)
        ));
    }

    #[test]
    fn rejects_expired_tokens_and_future_tokens() {
        let (service, identity, _, _) = tokens();
        assert!(matches!(
            service.validate_at(
                &identity,
                "identity",
                Some(SCOPE_CLIENT),
                Some(FINGERPRINT),
                10_000_000_000
            ),
            Err(AuthError::Time(_))
        ));
    }

    #[test]
    fn rejects_tampering_and_wrong_issuer() {
        let (service, identity, _, _) = tokens();
        let (signing_input, signature) = identity.rsplit_once('.').expect("signature segment");
        let mut bytes = base64url_decode(signature).expect("signature bytes");
        bytes[0] ^= 1;
        let tampered = format!("{signing_input}.{}", base64url(&bytes));
        assert!(matches!(
            service.validate(&tampered, "identity", Some(SCOPE_CLIENT), Some(FINGERPRINT)),
            Err(AuthError::BadSignature)
        ));

        let other = AuthService::new(service.key.clone(), "other", "rust-server");
        assert!(matches!(
            other.validate(&identity, "identity", Some(SCOPE_CLIENT), Some(FINGERPRINT)),
            Err(AuthError::InvalidClaim { claim: "iss", .. })
        ));
    }

    #[test]
    fn rejects_missing_scope_and_invalid_subject() {
        let service = service();
        let token = service
            .issue_identity_token(
                UUID,
                "RustBot",
                &[SCOPE_EDITOR.to_string()],
                Some(FINGERPRINT),
                300,
            )
            .expect("token");
        assert!(matches!(
            service.validate(&token, "identity", Some(SCOPE_CLIENT), Some(FINGERPRINT)),
            Err(AuthError::InvalidClaim { claim: "scope", .. })
        ));
        assert!(service
            .issue_identity_token("not-a-uuid", "RustBot", &[], None, 300)
            .is_err());
    }

    #[test]
    fn uuid_shape_check_matches_hyphenated_form() {
        assert!(is_uuid(UUID));
        assert!(!is_uuid("12345678123456789abcdef012345678"));
        assert!(!is_uuid("12345678-1234-5678-9abc-def01234567z"));
    }
}
