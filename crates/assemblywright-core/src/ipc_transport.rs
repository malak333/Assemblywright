#[cfg(target_os = "macos")]
use crate::macos_code_identity::{CompiledPeerRequirement, PeerAuditToken};
#[cfg(target_os = "macos")]
use crate::startup::validate_peer_code_requirement;
use crate::startup::{validate_unix_socket_path, PeerIdentityProfile};
use anyhow::{anyhow, bail, Context};
use axum::body::{to_bytes, Body};
use axum::http::{header, HeaderValue, Method, Request, Uri};
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tower::ServiceExt;

pub const UNIX_IPC_FRAME_VERSION: u16 = 1;
/// Encoded client frame cap. The decoded request body has the lower cap below.
pub const MAX_UNIX_IPC_REQUEST_FRAME_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_UNIX_IPC_REQUEST_BODY_BYTES: usize = 1024 * 1024;
/// Decoded router response cap. The larger frame cap accommodates Base64 expansion.
pub const MAX_UNIX_IPC_RESPONSE_BODY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_UNIX_IPC_RESPONSE_FRAME_BYTES: usize = 12 * 1024 * 1024;
pub const MAX_UNIX_IPC_CONNECTIONS: usize = 32;
pub const MAX_UNIX_IPC_PATH_AND_QUERY_BYTES: usize = 8 * 1024;
pub const MAX_UNIX_IPC_REQUEST_HEADER_VALUE_BYTES: usize = 1024;
pub const MAX_UNIX_IPC_RESPONSE_CONTENT_TYPE_BYTES: usize = 256;
pub const UNIX_IPC_READ_TIMEOUT_SECONDS: u64 = 10;
pub const UNIX_IPC_PEER_IDENTITY_TIMEOUT_SECONDS: u64 = 45;
// This must outlive the protocol's ten-minute job bound plus agent-side
// process-group cleanup. Shorter client routes retain their own tighter bounds.
pub const UNIX_IPC_DISPATCH_TIMEOUT_SECONDS: u64 = 620;
pub const UNIX_IPC_WRITE_TIMEOUT_SECONDS: u64 = 10;
const UNIX_IPC_READ_TIMEOUT: Duration = Duration::from_secs(UNIX_IPC_READ_TIMEOUT_SECONDS);
const UNIX_IPC_PEER_IDENTITY_TIMEOUT: Duration =
    Duration::from_secs(UNIX_IPC_PEER_IDENTITY_TIMEOUT_SECONDS);
const UNIX_IPC_DISPATCH_TIMEOUT: Duration = Duration::from_secs(UNIX_IPC_DISPATCH_TIMEOUT_SECONDS);
const UNIX_IPC_WRITE_TIMEOUT: Duration = Duration::from_secs(UNIX_IPC_WRITE_TIMEOUT_SECONDS);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FramedRequest {
    version: u16,
    method: String,
    path: String,
    authorization: Value,
    accept: Value,
    content_type: Value,
    body_base64: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FramedResponse {
    version: u16,
    status: u16,
    content_type: Option<String>,
    body_base64: String,
}

#[derive(Clone, Copy)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

impl FileIdentity {
    fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }

    fn matches(self, metadata: &std::fs::Metadata) -> bool {
        self.device == metadata.dev() && self.inode == metadata.ino()
    }
}

struct KnownSocketCleanup {
    path: PathBuf,
    identity: FileIdentity,
}

impl Drop for KnownSocketCleanup {
    fn drop(&mut self) {
        let Ok(metadata) = std::fs::symlink_metadata(&self.path) else {
            return;
        };
        if metadata.file_type().is_socket() && self.identity.matches(&metadata) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Serves a caller-supplied Axum router over the same owner-only, same-EUID,
/// Apple audit-token-bound Unix transport used by the app-supervised core.
/// The caller remains responsible for applying per-route bearer authorization.
#[cfg(target_os = "macos")]
pub async fn serve_router_unix_socket_with_peer_identity(
    socket_path: impl AsRef<Path>,
    app: Router,
    peer_code_requirement: &str,
    peer_identity_profile: PeerIdentityProfile,
) -> anyhow::Result<()> {
    validate_peer_code_requirement(peer_code_requirement, peer_identity_profile)
        .map_err(anyhow::Error::new)?;
    let requirement = Arc::new(CompiledPeerRequirement::compile(
        peer_code_requirement,
        peer_identity_profile,
    )?);
    serve_unix_socket_inner(
        socket_path.as_ref(),
        app,
        Some(requirement),
        #[cfg(test)]
        None,
    )
    .await
}

#[cfg(not(target_os = "macos"))]
pub async fn serve_router_unix_socket_with_peer_identity(
    _socket_path: impl AsRef<Path>,
    _app: Router,
    _peer_code_requirement: &str,
    _peer_identity_profile: PeerIdentityProfile,
) -> anyhow::Result<()> {
    bail!("Apple peer code identity verification is supported only on macOS")
}

async fn serve_unix_socket_inner(
    socket_path: &Path,
    app: Router,
    #[cfg(target_os = "macos")] peer_requirement: Option<Arc<CompiledPeerRequirement>>,
    #[cfg(test)] accepted_connections: Option<Arc<AtomicUsize>>,
) -> anyhow::Result<()> {
    let (listener, _cleanup) = bind_secure_unix_listener(socket_path)?;
    let permits = Arc::new(Semaphore::new(MAX_UNIX_IPC_CONNECTIONS));
    let mut shutdown = Box::pin(unix_shutdown_signal());

    loop {
        let permit = tokio::select! {
            result = Arc::clone(&permits).acquire_owned() => {
                result.map_err(|_| anyhow!("Unix IPC concurrency limiter closed"))?
            }
            result = &mut shutdown => {
                result?;
                break;
            }
        };
        let stream = tokio::select! {
            result = listener.accept() => result?.0,
            result = &mut shutdown => {
                result?;
                break;
            }
        };
        #[cfg(test)]
        if let Some(accepted_connections) = accepted_connections.as_ref() {
            accepted_connections.fetch_add(1, Ordering::SeqCst);
        }
        let app = app.clone();
        #[cfg(target_os = "macos")]
        let peer_requirement = peer_requirement.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(error) = handle_connection(
                stream,
                app,
                #[cfg(target_os = "macos")]
                peer_requirement,
            )
            .await
            {
                tracing::warn!(error = %error, "rejected Unix IPC connection");
            }
        });
    }
    Ok(())
}

async fn unix_shutdown_signal() -> anyhow::Result<()> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = interrupt.recv() => {}
        _ = terminate.recv() => {}
    }
    Ok(())
}

fn bind_secure_unix_listener(
    socket_path: &Path,
) -> anyhow::Result<(UnixListener, KnownSocketCleanup)> {
    validate_unix_socket_path(socket_path).map_err(anyhow::Error::new)?;
    let parent = socket_path
        .parent()
        .ok_or_else(|| anyhow!("Unix IPC socket path must have a parent"))?;
    let parent_metadata = std::fs::symlink_metadata(parent)
        .context("Unix IPC parent directory could not be inspected")?;
    let current_euid = unsafe { libc::geteuid() };
    if !parent_metadata.file_type().is_dir()
        || parent_metadata.uid() != current_euid
        || parent_metadata.permissions().mode() & 0o7777 != 0o700
    {
        bail!("Unix IPC parent directory must be an owner-matched mode-0700 directory");
    }
    let parent_identity = FileIdentity::from_metadata(&parent_metadata);
    match std::fs::symlink_metadata(socket_path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("Unix IPC socket leaf could not be inspected"),
        Ok(_) => bail!("Unix IPC socket leaf must not already exist"),
    }

    let listener = UnixListener::bind(socket_path).context("Unix IPC socket bind failed")?;
    let created_metadata = std::fs::symlink_metadata(socket_path)
        .context("created Unix IPC socket could not be inspected")?;
    if !created_metadata.file_type().is_socket() || created_metadata.uid() != current_euid {
        bail!("created Unix IPC leaf is not an owner-matched socket");
    }
    let cleanup = KnownSocketCleanup {
        path: socket_path.to_path_buf(),
        identity: FileIdentity::from_metadata(&created_metadata),
    };
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))
        .context("Unix IPC socket permissions could not be restricted")?;

    let final_parent = std::fs::symlink_metadata(parent)
        .context("Unix IPC parent directory could not be revalidated")?;
    let final_socket = std::fs::symlink_metadata(socket_path)
        .context("Unix IPC socket could not be revalidated")?;
    if !parent_identity.matches(&final_parent)
        || !final_socket.file_type().is_socket()
        || !cleanup.identity.matches(&final_socket)
        || final_socket.uid() != current_euid
        || final_socket.permissions().mode() & 0o7777 != 0o600
    {
        bail!("Unix IPC socket path changed or permissions were not enforced");
    }
    Ok((listener, cleanup))
}

async fn handle_connection(
    mut stream: UnixStream,
    app: Router,
    #[cfg(target_os = "macos")] peer_requirement: Option<Arc<CompiledPeerRequirement>>,
) -> anyhow::Result<()> {
    require_current_euid_peer(&stream)?;
    #[cfg(target_os = "macos")]
    if let Some(requirement) = peer_requirement {
        let token = require_peer_audit_token(&stream)?;
        timeout(
            UNIX_IPC_PEER_IDENTITY_TIMEOUT,
            tokio::task::spawn_blocking(move || requirement.verify(token)),
        )
        .await
        .map_err(|_| anyhow!("Unix IPC peer code identity validation timed out"))?
        .map_err(|_| anyhow!("Unix IPC peer code identity validation worker failed"))??;
    }
    let request_frame = read_frame(&mut stream, MAX_UNIX_IPC_REQUEST_FRAME_BYTES).await?;
    require_client_write_eof(&mut stream).await?;
    let response = dispatch_frame(&request_frame, app).await?;
    let response_frame = serde_json::to_vec(&response).context("serialize Unix IPC response")?;
    if response_frame.len() > MAX_UNIX_IPC_RESPONSE_FRAME_BYTES {
        bail!("Unix IPC response frame exceeds its size limit");
    }
    write_frame(&mut stream, &response_frame).await?;
    timeout(UNIX_IPC_WRITE_TIMEOUT, stream.shutdown())
        .await
        .map_err(|_| anyhow!("Unix IPC shutdown timed out"))??;
    Ok(())
}

#[cfg(target_os = "macos")]
fn require_current_euid_peer(stream: &UnixStream) -> anyhow::Result<()> {
    let mut peer_uid: libc::uid_t = 0;
    let mut peer_gid: libc::gid_t = 0;
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut peer_uid, &mut peer_gid) };
    if result != 0 {
        return Err(io::Error::last_os_error()).context("Unix IPC peer identity unavailable");
    }
    validate_peer_euid(peer_uid, unsafe { libc::geteuid() })
}

fn validate_peer_euid(peer_uid: libc::uid_t, current_euid: libc::uid_t) -> anyhow::Result<()> {
    if peer_uid != current_euid {
        bail!("Unix IPC peer effective UID does not match the server");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn require_peer_audit_token(stream: &UnixStream) -> anyhow::Result<PeerAuditToken> {
    let mut token = [0_u32; 8];
    let expected_length = std::mem::size_of_val(&token);
    let mut actual_length = libc::socklen_t::try_from(expected_length)
        .expect("Apple audit token length fits socklen_t");
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERTOKEN,
            token.as_mut_ptr().cast::<libc::c_void>(),
            &mut actual_length,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error()).context("Unix IPC peer audit token unavailable");
    }
    if actual_length as usize != expected_length {
        bail!("Unix IPC peer audit token has an invalid length");
    }
    Ok(token)
}

#[cfg(not(target_os = "macos"))]
fn require_current_euid_peer(_stream: &UnixStream) -> anyhow::Result<()> {
    bail!("Unix IPC peer verification is supported only on macOS")
}

async fn read_frame(stream: &mut UnixStream, maximum: usize) -> anyhow::Result<Vec<u8>> {
    let mut prefix = [0_u8; 4];
    timeout(UNIX_IPC_READ_TIMEOUT, stream.read_exact(&mut prefix))
        .await
        .map_err(|_| anyhow!("Unix IPC frame prefix timed out"))??;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > maximum {
        bail!("Unix IPC frame length is outside its allowed bounds");
    }
    let mut frame = vec![0_u8; length];
    timeout(UNIX_IPC_READ_TIMEOUT, stream.read_exact(&mut frame))
        .await
        .map_err(|_| anyhow!("Unix IPC frame body timed out"))??;
    Ok(frame)
}

async fn require_client_write_eof(stream: &mut UnixStream) -> anyhow::Result<()> {
    require_client_write_eof_with_timeout(stream, UNIX_IPC_READ_TIMEOUT).await
}

async fn require_client_write_eof_with_timeout(
    stream: &mut UnixStream,
    wait: Duration,
) -> anyhow::Result<()> {
    let mut trailing = [0_u8; 1];
    match timeout(wait, stream.read(&mut trailing)).await {
        Err(_) => bail!("Unix IPC client write-half EOF timed out"),
        Ok(Err(error)) => Err(error).context("Unix IPC client write-half EOF could not be read"),
        Ok(Ok(0)) => Ok(()),
        Ok(Ok(_)) => bail!("Unix IPC request contains trailing data"),
    }
}

async fn write_frame(stream: &mut UnixStream, frame: &[u8]) -> anyhow::Result<()> {
    let length = u32::try_from(frame.len()).context("Unix IPC response frame is too large")?;
    timeout(UNIX_IPC_WRITE_TIMEOUT, async {
        stream.write_all(&length.to_be_bytes()).await?;
        stream.write_all(frame).await
    })
    .await
    .map_err(|_| anyhow!("Unix IPC response write timed out"))??;
    Ok(())
}

async fn dispatch_frame(frame: &[u8], app: Router) -> anyhow::Result<FramedResponse> {
    let framed: FramedRequest =
        serde_json::from_slice(frame).context("Unix IPC request JSON is invalid")?;
    if framed.version != UNIX_IPC_FRAME_VERSION {
        bail!("Unix IPC request version is unsupported");
    }
    let method = match framed.method.as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "DELETE" => Method::DELETE,
        "PATCH" => Method::PATCH,
        _ => bail!("Unix IPC request method is not allowed"),
    };
    if framed.path.is_empty()
        || framed.path.len() > MAX_UNIX_IPC_PATH_AND_QUERY_BYTES
        || !framed.path.starts_with('/')
        || framed.path.contains("//")
        || framed.path.contains('#')
        || framed.path.bytes().any(|byte| byte.is_ascii_control())
    {
        bail!("Unix IPC request path is invalid");
    }
    let uri: Uri = framed
        .path
        .parse()
        .context("Unix IPC request path is not valid origin-form URI")?;
    if uri.scheme().is_some() || uri.authority().is_some() {
        bail!("Unix IPC request path must be origin-form");
    }
    let body = STANDARD
        .decode(framed.body_base64.as_bytes())
        .context("Unix IPC request body_base64 is invalid")?;
    if body.len() > MAX_UNIX_IPC_REQUEST_BODY_BYTES {
        bail!("Unix IPC request body exceeds its size limit");
    }

    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::from(body))
        .context("build Unix IPC router request")?;
    insert_optional_header(
        request.headers_mut(),
        header::AUTHORIZATION,
        required_nullable_string(framed.authorization, "authorization")?,
    )?;
    insert_optional_header(
        request.headers_mut(),
        header::ACCEPT,
        required_nullable_string(framed.accept, "accept")?,
    )?;
    insert_optional_header(
        request.headers_mut(),
        header::CONTENT_TYPE,
        required_nullable_string(framed.content_type, "content_type")?,
    )?;

    timeout(UNIX_IPC_DISPATCH_TIMEOUT, async move {
        let response = app
            .oneshot(request)
            .await
            .expect("Axum router is infallible");
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        if content_type
            .as_ref()
            .is_some_and(|value| value.len() > MAX_UNIX_IPC_RESPONSE_CONTENT_TYPE_BYTES)
        {
            bail!("Unix IPC response content type exceeds its size limit");
        }
        let response_body = to_bytes(response.into_body(), MAX_UNIX_IPC_RESPONSE_BODY_BYTES)
            .await
            .context("Unix IPC router response exceeds its body limit")?;
        Ok::<_, anyhow::Error>(FramedResponse {
            version: UNIX_IPC_FRAME_VERSION,
            status,
            content_type,
            body_base64: STANDARD.encode(response_body),
        })
    })
    .await
    .map_err(|_| anyhow!("Unix IPC request dispatch timed out"))?
}

fn required_nullable_string(value: Value, field: &str) -> anyhow::Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value)),
        _ => bail!("Unix IPC request {field} must be a string or null"),
    }
}

fn insert_optional_header(
    headers: &mut axum::http::HeaderMap,
    name: axum::http::HeaderName,
    value: Option<String>,
) -> anyhow::Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.len() > MAX_UNIX_IPC_REQUEST_HEADER_VALUE_BYTES {
        bail!("Unix IPC request header value exceeds its size limit");
    }
    let value =
        HeaderValue::from_str(&value).context("Unix IPC request header value is invalid")?;
    headers.insert(name, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    /// Minimal bearer-protected router standing in for a real consumer's
    /// router. The transport must preserve its middleware and status codes.
    fn test_router_with_auth(token: &'static str) -> Router {
        Router::new()
            .route(
                "/health",
                axum::routing::get(|| async { axum::Json(json!({ "status": "ok" })) }),
            )
            .layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| async move {
                    let expected = format!("Bearer {token}");
                    let presented = request
                        .headers()
                        .get(header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_string();
                    if presented != expected {
                        return axum::response::IntoResponse::into_response(
                            axum::http::StatusCode::UNAUTHORIZED,
                        );
                    }
                    next.run(request).await
                },
            ))
    }

    #[test]
    fn concurrency_contract_remains_bounded() {
        assert_eq!(MAX_UNIX_IPC_CONNECTIONS, 32);
        assert_eq!(UNIX_IPC_DISPATCH_TIMEOUT_SECONDS, 620);
    }

    #[tokio::test(start_paused = true)]
    async fn dispatch_bound_outlives_maximum_job_runtime() {
        let app = Router::new().route(
            "/slow",
            axum::routing::get(|| async {
                tokio::time::sleep(Duration::from_secs(601)).await;
                axum::http::StatusCode::OK
            }),
        );
        let request = json!({
            "version": UNIX_IPC_FRAME_VERSION,
            "method": "GET",
            "path": "/slow",
            "authorization": null,
            "accept": null,
            "content_type": null,
            "body_base64": ""
        });

        let response = dispatch_frame(request.to_string().as_bytes(), app)
            .await
            .expect("601-second simulated dispatch stays inside cleanup-aware bound");
        assert_eq!(response.status, axum::http::StatusCode::OK.as_u16());
    }

    #[test]
    fn frame_schema_requires_nullable_fields_and_rejects_unknown_fields() {
        let missing = json!({
            "version": 1,
            "method": "GET",
            "path": "/health",
            "accept": null,
            "content_type": null,
            "body_base64": ""
        });
        assert!(serde_json::from_value::<FramedRequest>(missing).is_err());

        let unknown = json!({
            "version": 1,
            "method": "GET",
            "path": "/health",
            "authorization": null,
            "accept": null,
            "content_type": null,
            "body_base64": "",
            "headers": {}
        });
        assert!(serde_json::from_value::<FramedRequest>(unknown).is_err());
    }

    #[tokio::test]
    async fn dispatch_whitelists_methods_and_preserves_auth_middleware() {
        let app = test_router_with_auth(TEST_TOKEN);
        let request = |method: &str, authorization: Option<&str>| {
            json!({
                "version": 1,
                "method": method,
                "path": "/health",
                "authorization": authorization,
                "accept": "application/json",
                "content_type": "application/json",
                "body_base64": ""
            })
        };

        let authorized = dispatch_frame(
            request("GET", Some(&format!("Bearer {TEST_TOKEN}")))
                .to_string()
                .as_bytes(),
            app.clone(),
        )
        .await
        .expect("authorized response");
        assert_eq!(authorized.status, 200);

        let unauthorized = dispatch_frame(request("GET", None).to_string().as_bytes(), app.clone())
            .await
            .expect("unauthorized response");
        assert_eq!(unauthorized.status, 401);

        assert!(dispatch_frame(
            request("PUT", Some(&format!("Bearer {TEST_TOKEN}")))
                .to_string()
                .as_bytes(),
            app,
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn dispatch_rejects_non_origin_or_control_bearing_paths() {
        let app = test_router_with_auth(TEST_TOKEN);
        for path in [
            "//host/health",
            "/health#fragment",
            "/health\nnext",
            "/health\0next",
        ] {
            let request = json!({
                "version": 1,
                "method": "GET",
                "path": path,
                "authorization": format!("Bearer {TEST_TOKEN}"),
                "accept": "application/json",
                "content_type": "application/json",
                "body_base64": ""
            });
            assert!(
                dispatch_frame(request.to_string().as_bytes(), app.clone())
                    .await
                    .is_err(),
                "accepted unsafe path {path:?}"
            );
        }
    }

    #[tokio::test]
    async fn frame_reader_rejects_oversized_declared_length() {
        let (mut writer, mut reader) = UnixStream::pair().expect("socket pair");
        writer
            .write_all(&((MAX_UNIX_IPC_REQUEST_FRAME_BYTES + 1) as u32).to_be_bytes())
            .await
            .expect("write prefix");
        assert!(read_frame(&mut reader, MAX_UNIX_IPC_REQUEST_FRAME_BYTES)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn request_boundary_requires_eof_and_rejects_trailing_or_stalled_writers() {
        let (mut writer, mut reader) = UnixStream::pair().expect("socket pair");
        writer.shutdown().await.expect("half-close writer");
        require_client_write_eof_with_timeout(&mut reader, Duration::from_millis(100))
            .await
            .expect("accept EOF");

        let (mut writer, mut reader) = UnixStream::pair().expect("socket pair");
        writer.write_all(b"x").await.expect("write trailing byte");
        writer.shutdown().await.expect("half-close writer");
        assert!(
            require_client_write_eof_with_timeout(&mut reader, Duration::from_millis(100))
                .await
                .is_err()
        );

        let (_writer, mut reader) = UnixStream::pair().expect("socket pair");
        assert!(
            require_client_write_eof_with_timeout(&mut reader, Duration::from_millis(10))
                .await
                .is_err()
        );
    }

    #[test]
    fn peer_euid_validation_rejects_mismatch() {
        assert!(validate_peer_euid(41, 42).is_err());
        assert!(validate_peer_euid(42, 42).is_ok());
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn peer_code_identity_is_checked_before_any_frame_read() {
        let (_client, server) = UnixStream::pair().expect("socket pair");
        let requirement = Arc::new(
            CompiledPeerRequirement::compile(
                "identifier \"com.example.assemblywright.never\"",
                PeerIdentityProfile::AdhocExact,
            )
            .expect("compile rejecting requirement"),
        );
        let result = timeout(
            UNIX_IPC_PEER_IDENTITY_TIMEOUT + Duration::from_secs(2),
            handle_connection(server, test_router_with_auth(TEST_TOKEN), Some(requirement)),
        )
        .await
        .expect("identity rejection must complete within the bounded identity timeout")
        .expect_err("reject mismatched peer code identity");
        assert!(result.to_string().contains("OSStatus"), "{result}");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn local_peer_token_resolves_to_current_valid_code() {
        let (_client, server) = UnixStream::pair().expect("socket pair");
        let token = require_peer_audit_token(&server).expect("retrieve peer audit token");
        CompiledPeerRequirement::compile("true", PeerIdentityProfile::AdhocExact)
            .expect("compile accepting requirement")
            .verify(token)
            .expect("validate current process from audit token");
        let hardened_error =
            CompiledPeerRequirement::compile("true", PeerIdentityProfile::DeveloperIdHardened)
                .expect("compile hardened requirement")
                .verify(token)
                .expect_err("linker-signed test binary must not satisfy hardened profile");
        assert!(
            hardened_error.to_string().contains("hardened runtime"),
            "{hardened_error}"
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn unix_socket_routes_one_framed_request_and_cleans_known_leaf() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let parent = temporary.path().join("ipc");
        std::fs::create_dir(&parent).expect("create IPC parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
            .expect("secure IPC parent");
        let socket_path = parent.join("core.sock");
        let server_path = socket_path.clone();
        let server = tokio::spawn(async move {
            serve_unix_socket_inner(&server_path, test_router_with_auth(TEST_TOKEN), None, None)
                .await
        });
        for _ in 0..100 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let request = json!({
            "version": 1,
            "method": "GET",
            "path": "/health",
            "authorization": format!("Bearer {TEST_TOKEN}"),
            "accept": "application/json",
            "content_type": "application/json",
            "body_base64": ""
        });

        let mut rejected = UnixStream::connect(&socket_path).await.expect("connect");
        write_frame(&mut rejected, request.to_string().as_bytes())
            .await
            .expect("write declared request");
        rejected
            .write_all(b"trailing")
            .await
            .expect("write trailing data");
        rejected
            .shutdown()
            .await
            .expect("half-close rejected writer");
        let mut rejected_response = [0_u8; 1];
        assert_eq!(
            timeout(
                Duration::from_secs(1),
                rejected.read(&mut rejected_response)
            )
            .await
            .expect("server must close rejected request")
            .expect("read rejected request"),
            0
        );

        let mut stream = UnixStream::connect(&socket_path).await.expect("connect");
        write_frame(&mut stream, request.to_string().as_bytes())
            .await
            .expect("write request");
        stream.shutdown().await.expect("half-close request writer");
        let response = read_frame(&mut stream, MAX_UNIX_IPC_RESPONSE_FRAME_BYTES)
            .await
            .expect("read response");
        let response: FramedResponse = serde_json::from_slice(&response).expect("response JSON");
        assert_eq!(response.status, 200);
        assert!(STANDARD.decode(response.body_base64).is_ok());

        server.abort();
        let _ = server.await;
        for _ in 0..100 {
            if !socket_path.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("known socket leaf was not cleaned up");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn unix_socket_queues_connection_beyond_live_concurrency_bound() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let parent = temporary.path().join("ipc");
        std::fs::create_dir(&parent).expect("create IPC parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
            .expect("secure IPC parent");
        let socket_path = parent.join("core.sock");
        let server_path = socket_path.clone();
        let accepted_connections = Arc::new(AtomicUsize::new(0));
        let server_accepted_connections = Arc::clone(&accepted_connections);
        let server = tokio::spawn(async move {
            serve_unix_socket_inner(
                &server_path,
                test_router_with_auth(TEST_TOKEN),
                None,
                Some(server_accepted_connections),
            )
            .await
        });
        for _ in 0..200 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(socket_path.exists(), "server socket was not created");

        let mut blockers = Vec::with_capacity(MAX_UNIX_IPC_CONNECTIONS);
        for _ in 0..MAX_UNIX_IPC_CONNECTIONS {
            blockers.push(
                UnixStream::connect(&socket_path)
                    .await
                    .expect("blocker connect"),
            );
        }
        for _ in 0..200 {
            if accepted_connections.load(Ordering::SeqCst) == MAX_UNIX_IPC_CONNECTIONS {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            accepted_connections.load(Ordering::SeqCst),
            MAX_UNIX_IPC_CONNECTIONS,
            "server did not admit all bounded blocker connections"
        );

        let request = json!({
            "version": 1,
            "method": "GET",
            "path": "/health",
            "authorization": format!("Bearer {TEST_TOKEN}"),
            "accept": "application/json",
            "content_type": "application/json",
            "body_base64": ""
        });
        let mut queued = UnixStream::connect(&socket_path)
            .await
            .expect("queued connection");
        write_frame(&mut queued, request.to_string().as_bytes())
            .await
            .expect("write queued request");
        queued.shutdown().await.expect("half-close queued writer");
        assert!(
            timeout(Duration::from_millis(100), queued.readable())
                .await
                .is_err(),
            "queued request became readable while all permits were held"
        );
        assert_eq!(
            accepted_connections.load(Ordering::SeqCst),
            MAX_UNIX_IPC_CONNECTIONS,
            "server admitted a connection beyond the configured bound"
        );

        drop(blockers.pop());
        for _ in 0..200 {
            if accepted_connections.load(Ordering::SeqCst) == MAX_UNIX_IPC_CONNECTIONS + 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            accepted_connections.load(Ordering::SeqCst),
            MAX_UNIX_IPC_CONNECTIONS + 1,
            "queued request was not admitted after a permit was released"
        );
        let response = timeout(
            Duration::from_secs(1),
            read_frame(&mut queued, MAX_UNIX_IPC_RESPONSE_FRAME_BYTES),
        )
        .await
        .expect("queued response timed out")
        .expect("queued response frame");
        let response: FramedResponse = serde_json::from_slice(&response).expect("response JSON");
        assert_eq!(response.status, 200);

        drop(blockers);
        drop(queued);
        server.abort();
        let _ = server.await;
        for _ in 0..100 {
            if !socket_path.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("known socket leaf was not cleaned up");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn binding_requires_secure_parent_and_absent_leaf() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let parent = temporary.path().join("ipc");
        std::fs::create_dir(&parent).expect("create IPC parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755))
            .expect("set insecure mode");
        let socket_path = parent.join("core.sock");
        assert!(bind_secure_unix_listener(&socket_path).is_err());

        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
            .expect("set secure mode");
        std::fs::write(&socket_path, b"replacement").expect("create existing leaf");
        assert!(bind_secure_unix_listener(&socket_path).is_err());
        assert_eq!(
            std::fs::read(&socket_path).expect("existing leaf"),
            b"replacement"
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn cleanup_never_unlinks_a_replacement_leaf() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let parent = temporary.path().join("ipc");
        std::fs::create_dir(&parent).expect("create IPC parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
            .expect("secure IPC parent");
        let socket_path = parent.join("core.sock");
        let (listener, cleanup) = bind_secure_unix_listener(&socket_path).expect("bind socket");
        std::fs::remove_file(&socket_path).expect("remove known socket leaf");
        std::fs::write(&socket_path, b"replacement").expect("create replacement leaf");
        drop(listener);
        drop(cleanup);
        assert_eq!(
            std::fs::read(&socket_path).expect("replacement"),
            b"replacement"
        );
    }
}
