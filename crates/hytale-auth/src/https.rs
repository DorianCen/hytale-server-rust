//! Small blocking HTTPS client for the live session services.
//!
//! `ureq` only exposes root-certificate configuration, not a custom
//! `ServerCertVerifier`, so the auth client uses rustls directly. The verifier
//! wraps the normal WebPKI verifier and rejects the eight compromised eMudhra
//! intermediate CA keys that Java's `CertificateTrustPolicy` denies.
//!
//! Scope is deliberately small: HTTP/1.1, `Connection: close`, JSON/form
//! bodies and content-length/chunked responses. The live endpoints do not need
//! redirects, cookies or connection reuse.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, Error, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::{AuthError, Result};

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

/// The eight compromised eMudhra CA keys from
/// `CoreServer/.../security/CertificateTrustPolicy.java` (SPKI SHA-256,
/// uppercase hex).
pub const DISTRUSTED_SPKI_SHA256: [&str; 8] = [
    "EBED6475C2C2D80FCB199F3FE8DD6E6B3CE60B77EB15D4E42C3DEC0C6F5B5105",
    "43BFFA19D6EF651451F6841E019931E96F6F2EAEFF28AB4B6F1CE4576DE2717B",
    "E0254C7FCDC265E2F0F8A428DABB75AB4AE039B4872D62F5EE57F0877D5A8A41",
    "49DD353DC2B919A3770645117D0E60C16115E7D1BC259EEFE734B00EA434FE5F",
    "63ECEFEE431D49523BC1136C4E72D0206A477086CAF606CD5CF025686C109CF8",
    "5C0660A2B164910D8FF589C18E14552E9E0EC237DCEFE2CB736D41A4811F81A0",
    "EC4FFA1AD0AC88081314B7AAFFEF6A91F97CD57291BB5BA686C5415F5EB4F6B6",
    "92C063D67E8F14DB71D288E11266FFDAF69CBF115F7BAB14F3BC2F68D25E0E30",
];

/// One HTTP response returned to the auth service layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body.
    pub body: Vec<u8>,
}

impl HttpResponse {
    #[cfg(test)]
    fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Blocking HTTPS client used for OAuth/session-service calls.
pub struct HttpsClient {
    tls: Arc<ClientConfig>,
    timeout: Duration,
    user_agent: String,
}

impl HttpsClient {
    /// Build the client with the Java-compatible outbound trust policy.
    pub fn new(timeout: Duration, user_agent: String) -> Result<Self> {
        let denied = DISTRUSTED_SPKI_SHA256
            .iter()
            .map(|hash| (*hash).to_string())
            .collect();
        Ok(Self {
            tls: build_tls_config(denied)?,
            timeout,
            user_agent,
        })
    }

    /// `GET` with an optional bearer token.
    pub fn get(&self, url: &str, bearer: Option<&str>) -> Result<HttpResponse> {
        self.request("GET", url, bearer, None)
    }

    /// `POST` an `application/x-www-form-urlencoded` body.
    pub fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<HttpResponse> {
        let body = serde_urlencoded(form);
        self.request(
            "POST",
            url,
            None,
            Some(("application/x-www-form-urlencoded", body)),
        )
    }

    /// `POST` a JSON body with an optional bearer token.
    pub fn post_json(
        &self,
        url: &str,
        bearer: Option<&str>,
        value: &serde_json::Value,
    ) -> Result<HttpResponse> {
        let body = serde_json::to_vec(value)?;
        self.request("POST", url, bearer, Some(("application/json", body)))
    }

    /// `POST` with no body.
    pub fn post_empty(&self, url: &str, bearer: Option<&str>) -> Result<HttpResponse> {
        self.request("POST", url, bearer, Some(("application/json", Vec::new())))
    }

    fn request(
        &self,
        method: &str,
        url: &str,
        bearer: Option<&str>,
        body: Option<(&str, Vec<u8>)>,
    ) -> Result<HttpResponse> {
        let parsed = url::Url::parse(url)
            .map_err(|error| AuthError::Service(format!("invalid URL `{url}`: {error}")))?;
        let scheme = parsed.scheme();
        let secure = match scheme {
            "https" => true,
            "http" if is_loopback_host(parsed.host_str().unwrap_or_default()) => false,
            _ => {
                return Err(AuthError::Service(format!(
                    "refusing non-HTTPS auth URL `{url}`"
                )))
            }
        };
        let host = parsed
            .host_str()
            .ok_or_else(|| AuthError::Service(format!("URL `{url}` has no host")))?;
        let port = parsed
            .port_or_known_default()
            .ok_or_else(|| AuthError::Service(format!("URL `{url}` has no port")))?;
        let host_header = if parsed.port().is_some() && parsed.port() != Some(443) {
            format!("{host}:{port}")
        } else {
            host.to_string()
        };
        let mut path = parsed.path().to_string();
        if let Some(query) = parsed.query() {
            path.push('?');
            path.push_str(query);
        }

        let mut stream = self.connect(host, port, secure)?;
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: {}\r\n\
             Accept: application/json\r\nConnection: close\r\n",
            self.user_agent
        );
        if let Some(token) = bearer {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        if let Some((content_type, payload)) = &body {
            request.push_str(&format!(
                "Content-Type: {content_type}\r\nContent-Length: {}\r\n",
                payload.len()
            ));
        }
        request.push_str("\r\n");

        stream
            .write_all(request.as_bytes())
            .map_err(|error| AuthError::Service(format!("auth request write failed: {error}")))?;
        if let Some((_, payload)) = body {
            stream.write_all(&payload).map_err(|error| {
                AuthError::Service(format!("auth request body write failed: {error}"))
            })?;
        }
        stream
            .flush()
            .map_err(|error| AuthError::Service(format!("auth request flush failed: {error}")))?;

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(|error| AuthError::Service(format!("auth response read failed: {error}")))?;
        parse_response(&response)
    }

    fn connect(&self, host: &str, port: u16, secure: bool) -> Result<Box<dyn ReadWrite>> {
        let addresses = (host, port)
            .to_socket_addrs()
            .map_err(|error| AuthError::Service(format!("cannot resolve `{host}`: {error}")))?;
        let mut last_error = None;
        let mut socket = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, self.timeout) {
                Ok(stream) => {
                    socket = Some(stream);
                    break;
                }
                Err(error) => last_error = Some(error),
            }
        }
        let socket = socket.ok_or_else(|| {
            AuthError::Service(format!(
                "cannot connect to `{host}`: {}",
                last_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "no address".to_string())
            ))
        })?;
        socket
            .set_read_timeout(Some(self.timeout))
            .map_err(|error| AuthError::Service(format!("socket timeout setup failed: {error}")))?;
        socket
            .set_write_timeout(Some(self.timeout))
            .map_err(|error| AuthError::Service(format!("socket timeout setup failed: {error}")))?;
        socket
            .set_nodelay(true)
            .map_err(|error| AuthError::Service(format!("socket setup failed: {error}")))?;

        let server_name = ServerName::try_from(host.to_string()).map_err(|error| {
            AuthError::Service(format!("invalid TLS server name `{host}`: {error}"))
        })?;
        if !secure {
            return Ok(Box::new(socket));
        }
        let connection = ClientConnection::new(Arc::clone(&self.tls), server_name)
            .map_err(|error| AuthError::Service(format!("TLS setup failed: {error}")))?;
        Ok(Box::new(rustls::StreamOwned::new(connection, socket)))
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
        || host == "[::1]"
}

fn serde_urlencoded(form: &[(&str, &str)]) -> Vec<u8> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(form.iter().copied());
    serializer.finish().into_bytes()
}

fn parse_response(bytes: &[u8]) -> Result<HttpResponse> {
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| AuthError::Service("auth response has no header terminator".to_string()))?;
    let header = std::str::from_utf8(&bytes[..header_end]).map_err(|error| {
        AuthError::Service(format!("auth response header is not UTF-8: {error}"))
    })?;
    let mut lines = header.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| AuthError::Service("auth response has no status code".to_string()))?;
    let mut chunked = false;
    let mut content_length = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "transfer-encoding" && value.eq_ignore_ascii_case("chunked") {
            chunked = true;
        } else if name == "content-length" {
            content_length = value.parse::<usize>().ok();
        }
    }

    let body = &bytes[header_end + 4..];
    let body = if chunked {
        decode_chunked(body)?
    } else if let Some(length) = content_length {
        if body.len() < length {
            return Err(AuthError::Service(format!(
                "auth response body truncated: {} of {length} bytes",
                body.len()
            )));
        }
        body[..length].to_vec()
    } else {
        body.to_vec()
    };
    Ok(HttpResponse { status, body })
}

fn decode_chunked(data: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut position = 0;
    loop {
        let line_end = data[position..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .map(|offset| position + offset)
            .ok_or_else(|| AuthError::Service("truncated chunked auth response".to_string()))?;
        let line = std::str::from_utf8(&data[position..line_end])
            .map_err(|error| AuthError::Service(format!("invalid chunk header: {error}")))?;
        let size_text = line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|error| AuthError::Service(format!("invalid chunk size: {error}")))?;
        position = line_end + 2;
        if size == 0 {
            return Ok(output);
        }
        let end = position
            .checked_add(size)
            .ok_or_else(|| AuthError::Service("chunk size overflow".to_string()))?;
        if end + 2 > data.len() {
            return Err(AuthError::Service(
                "truncated chunked auth response body".to_string(),
            ));
        }
        output.extend_from_slice(&data[position..end]);
        position = end + 2;
    }
}

fn build_tls_config(denied: HashSet<String>) -> Result<Arc<ClientConfig>> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    build_tls_config_with_roots(roots, denied)
}

fn build_tls_config_with_roots(
    roots: rustls::RootCertStore,
    denied: HashSet<String>,
) -> Result<Arc<ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&provider))
        .build()
        .map_err(|error| AuthError::Service(format!("TLS verifier setup failed: {error}")))?;
    let verifier = Arc::new(DistrustedKeyVerifier { inner, denied });
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| AuthError::Service(format!("TLS version setup failed: {error}")))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// A WebPKI verifier that rejects the Java deny-list before delegating.
#[derive(Debug)]
struct DistrustedKeyVerifier {
    inner: Arc<WebPkiServerVerifier>,
    denied: HashSet<String>,
}

impl ServerCertVerifier for DistrustedKeyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, Error> {
        for certificate in std::iter::once(end_entity).chain(intermediates.iter()) {
            if let Some(hash) = spki_sha256(certificate.as_ref()) {
                if self.denied.contains(&hash) {
                    return Err(Error::General(format!(
                        "certificate chain contains a distrusted eMudhra CA key \
                         (spkiSha256={hash})"
                    )));
                }
            }
        }
        self.inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// SHA-256 of a certificate's DER-encoded SubjectPublicKeyInfo, uppercase hex.
fn spki_sha256(certificate: &[u8]) -> Option<String> {
    let spki = subject_public_key_info(certificate)?;
    let digest = Sha256::digest(spki);
    Some(digest.iter().map(|byte| format!("{byte:02X}")).collect())
}

/// Extract the full `SubjectPublicKeyInfo` TLV from an X.509 certificate.
fn subject_public_key_info(certificate: &[u8]) -> Option<&[u8]> {
    // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signatureValue }
    let (_, outer_content, _) = read_tlv(certificate, 0, 0x30)?;
    // tbsCertificate ::= SEQUENCE { [0] version, serial, signature, issuer, validity, subject, spki, ... }
    let (_, tbs_content, tbs_end) = read_tlv(certificate, outer_content, 0x30)?;
    let mut position = tbs_content;
    // Optional explicit [0] version.
    if position < tbs_end && certificate[position] == 0xA0 {
        let (_, _, end) = read_tlv(certificate, position, 0xA0)?;
        position = end;
    }
    // serialNumber, signature, issuer, validity, subject.
    for _ in 0..5 {
        let (_, _, end) = read_tlv_any(certificate, position)?;
        position = end;
    }
    let (start, _, end) = read_tlv(certificate, position, 0x30)?;
    Some(&certificate[start..end])
}

fn read_tlv_any(data: &[u8], position: usize) -> Option<(usize, usize, usize)> {
    let tag = *data.get(position)?;
    read_tlv(data, position, tag)
}

fn read_tlv(data: &[u8], position: usize, expected_tag: u8) -> Option<(usize, usize, usize)> {
    if *data.get(position)? != expected_tag {
        return None;
    }
    let first_length = *data.get(position + 1)?;
    let (length, header_length) = if first_length & 0x80 == 0 {
        (usize::from(first_length), 2)
    } else {
        let count = usize::from(first_length & 0x7F);
        if count == 0 || count > 4 {
            return None;
        }
        let mut length = 0usize;
        for offset in 0..count {
            length = (length << 8) | usize::from(*data.get(position + 2 + offset)?);
        }
        (length, 2 + count)
    };
    let content_start = position.checked_add(header_length)?;
    let end = content_start.checked_add(length)?;
    if end > data.len() {
        return None;
    }
    Some((position, content_start, end))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::time::Duration;

    use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair, KeyUsagePurpose};
    use rustls::client::danger::ServerCertVerifier;
    use rustls::client::WebPkiServerVerifier;
    use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
    use rustls::RootCertStore;
    use sha2::{Digest, Sha256};

    use super::{
        build_tls_config_with_roots, decode_chunked, parse_response, spki_sha256,
        DistrustedKeyVerifier, HttpsClient, DISTRUSTED_SPKI_SHA256,
    };

    #[test]
    fn the_java_deny_list_is_the_expected_eight_hashes() {
        assert_eq!(DISTRUSTED_SPKI_SHA256.len(), 8);
        for hash in DISTRUSTED_SPKI_SHA256 {
            assert_eq!(hash.len(), 64);
            assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(hash.chars().all(|c| !c.is_ascii_lowercase()));
        }
    }

    #[test]
    fn chunked_and_content_length_responses_parse() {
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        let response = parse_response(chunked).expect("chunked");
        assert_eq!(response.status, 200);
        assert_eq!(response.body_text(), "{\"a\":1}");

        let sized = b"HTTP/1.1 400 Bad Request\r\nContent-Length: 13\r\n\r\n{\"error\":\"x\"}";
        let response = parse_response(sized).expect("content length");
        assert_eq!(response.status, 400);
        assert_eq!(response.body_text(), "{\"error\":\"x\"}");

        assert_eq!(
            decode_chunked(b"3\r\nabc\r\n0\r\n\r\n").expect("decode"),
            b"abc"
        );
        assert!(parse_response(b"not http").is_err());
    }

    fn test_chain() -> (rcgen::Certificate, rcgen::Certificate, rcgen::KeyPair) {
        let root_key = KeyPair::generate().expect("root key");
        let mut root_params = CertificateParams::new(vec!["test root".to_string()]).expect("root");
        root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        root_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let root = root_params.self_signed(&root_key).expect("root cert");

        let intermediate_key = KeyPair::generate().expect("intermediate key");
        let mut intermediate_params =
            CertificateParams::new(vec!["test intermediate".to_string()]).expect("intermediate");
        intermediate_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        intermediate_params.key_usages =
            vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let intermediate = intermediate_params
            .signed_by(&intermediate_key, &root, &root_key)
            .expect("intermediate cert");
        (root, intermediate, intermediate_key)
    }

    fn make_verifier(root: &rcgen::Certificate, denied: HashSet<String>) -> DistrustedKeyVerifier {
        let mut roots = RootCertStore::empty();
        roots.add(root.der().clone()).expect("trust root");
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
            .build()
            .expect("verifier");
        DistrustedKeyVerifier { inner, denied }
    }

    #[test]
    fn a_denied_intermediate_is_rejected_and_a_clean_chain_is_accepted() {
        let (root, intermediate, intermediate_key) = test_chain();
        let leaf_key = KeyPair::generate().expect("leaf key");
        let leaf_params = CertificateParams::new(vec!["localhost".to_string()]).expect("leaf");
        let leaf = leaf_params
            .signed_by(&leaf_key, &intermediate, &intermediate_key)
            .expect("leaf cert");
        let hash = spki_sha256(intermediate.der()).expect("spki hash");
        assert_eq!(hash.len(), 64);
        let expected = {
            let digest = Sha256::digest(intermediate_key.public_key_der());
            digest
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>()
        };
        assert_eq!(hash, expected, "DER walk must find the rcgen SPKI");

        let denied = HashSet::from([hash.clone()]);
        let checked = make_verifier(&root, denied);
        let error = checked
            .verify_server_cert(
                leaf.der(),
                &[intermediate.der().clone()],
                &ServerName::try_from("localhost").expect("name"),
                &[],
                UnixTime::now(),
            )
            .expect_err("the compromised intermediate must be rejected");
        assert!(
            error.to_string().contains("distrusted eMudhra"),
            "unexpected error: {error}"
        );

        let checked = make_verifier(&root, HashSet::new());
        assert!(checked
            .verify_server_cert(
                leaf.der(),
                &[intermediate.der().clone()],
                &ServerName::try_from("localhost").expect("name"),
                &[],
                UnixTime::now(),
            )
            .is_ok());
    }

    #[test]
    fn the_https_client_completes_a_tls_request() {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("self-signed cert");
        let cert_der = certified.cert.der().clone();
        let key_der = certified.key_pair.serialize_der();

        let mut roots = RootCertStore::empty();
        roots.add(cert_der.clone()).expect("trust anchor");
        let tls = build_tls_config_with_roots(roots, HashSet::new()).expect("client TLS");
        let client = HttpsClient {
            tls,
            timeout: Duration::from_secs(5),
            user_agent: "hytale-auth-test".to_string(),
        };

        let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![cert_der],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
        )
        .expect("server TLS");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS server");
        let port = listener.local_addr().expect("server address").port();
        let server = std::thread::spawn(move || {
            let (socket, _) = listener.accept().expect("accept TLS client");
            let connection =
                rustls::ServerConnection::new(Arc::new(server_config)).expect("server connection");
            let mut stream = rustls::StreamOwned::new(connection, socket);
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).expect("read request");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello")
                .expect("write response");
            stream.conn.send_close_notify();
            stream.flush().expect("flush response");
        });

        let response = client
            .get(&format!("https://localhost:{port}/hello"), None)
            .expect("HTTPS response");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"hello");
        server.join().expect("TLS server");
    }
}
