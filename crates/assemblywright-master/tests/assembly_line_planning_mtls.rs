#![cfg(windows)]

use assemblywright_master::{
    current_time_ms, EnrollmentGrantSpec, EnrollmentRequest, IdentityAuthority, MasterProcess,
    PlatformSecretProtector,
};
use assemblywright_protocol::{
    AssemblyLineAutoRunRequest, AssemblyLineOwnerProjection, AuthenticatedHandshakeRequest,
    CapabilityDescriptor, DeviceRole, HandshakeRequest, HandshakeResponse,
    FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION, PROTOCOL_VERSION,
};
use rcgen::{CertificateParams, DistinguishedName, DnType, IsCa, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;
use uuid::Uuid;

const TLS_EXPORTER_LABEL: &[u8] = b"EXPORTER-Assemblywright-Developer-Mode-v1";

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct EnrolledClient {
    config: Arc<ClientConfig>,
    handshake: HandshakeRequest,
}

#[tokio::test(flavor = "multi_thread")]
async fn designated_mac_mtls_is_required_for_inert_planning_routes() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    let setup = Command::new(binary)
        .arg("--data-dir")
        .arg(directory.path())
        .arg("setup")
        .output()
        .unwrap();
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let designated = enroll_client(
        directory.path(),
        "designated-planning-mac",
        DeviceRole::MacBridge,
        vec![CapabilityDescriptor::mlx_reasoning(
            "owner-planning",
            32 * 1024,
            32 * 1024,
        )],
    );
    let other = enroll_client(
        directory.path(),
        "other-planning-mac",
        DeviceRole::MacBridge,
        vec![CapabilityDescriptor::mlx_reasoning(
            "owner-planning",
            32 * 1024,
            32 * 1024,
        )],
    );
    let worker = enroll_client(
        directory.path(),
        "planning-denied-worker",
        DeviceRole::InferenceWorker,
        vec![CapabilityDescriptor::local_coding()],
    );
    {
        let mut process = MasterProcess::acquire(directory.path()).unwrap();
        process
            .kernel_mut()
            .designate_owner_control_bridge(designated.handshake.device_id, 0, 10)
            .unwrap();
    }
    let local_endpoint = unused_loopback_addr();
    let remote_endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), local_endpoint, remote_endpoint);
    read_ready(&mut server.0);

    let pre_handshake = one_shot_request(
        remote_endpoint,
        designated.config.clone(),
        "GET",
        "/v1/distributed/assembly-line",
        &[],
    )
    .await;
    assert!(
        pre_handshake.starts_with("HTTP/1.1 401 Unauthorized"),
        "{pre_handshake}"
    );

    for denied in [&other, &worker] {
        let response = authenticated_request(
            remote_endpoint,
            denied,
            "GET",
            "/v1/distributed/assembly-line",
            &serde_json::json!({}),
        )
        .await;
        assert!(
            response.starts_with("HTTP/1.1 401 Unauthorized"),
            "{response}"
        );
    }

    let projection_response = authenticated_request(
        remote_endpoint,
        &designated,
        "GET",
        "/v1/distributed/assembly-line",
        &serde_json::json!({}),
    )
    .await;
    assert!(
        projection_response.starts_with("HTTP/1.1 200 OK"),
        "{projection_response}"
    );
    let projection: AssemblyLineOwnerProjection = response_json(&projection_response);
    assert!(projection.assembly_line.auto_run);
    assert!(projection.queue.is_empty());

    let toggle = AssemblyLineAutoRunRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        expected_state_revision: projection.assembly_line.state_revision,
        auto_run: false,
    };
    let toggle_response = authenticated_request(
        remote_endpoint,
        &designated,
        "POST",
        "/v1/distributed/assembly-line/auto-run",
        &toggle,
    )
    .await;
    assert!(
        toggle_response.starts_with("HTTP/1.1 200 OK"),
        "{toggle_response}"
    );

    let forbidden_start = authenticated_request(
        remote_endpoint,
        &designated,
        "POST",
        "/v1/distributed/assembly-line/start",
        &serde_json::json!({}),
    )
    .await;
    assert!(
        forbidden_start.starts_with("HTTP/1.1 404 Not Found"),
        "{forbidden_start}"
    );
}

fn enroll_client(
    data_dir: &Path,
    name: &str,
    role: DeviceRole,
    capabilities: Vec<CapabilityDescriptor>,
) -> EnrolledClient {
    let now_ms = current_time_ms().unwrap();
    let protector = PlatformSecretProtector;
    let mut process = MasterProcess::acquire(data_dir).unwrap();
    let authority = IdentityAuthority::open_or_initialize(data_dir, &protector, now_ms).unwrap();
    process
        .kernel_mut()
        .record_identity_authority(authority.receipt())
        .unwrap();
    let grant = process
        .kernel_mut()
        .create_enrollment_grant(
            EnrollmentGrantSpec {
                device_name: name.to_string(),
                role,
                capabilities: capabilities.clone(),
            },
            now_ms,
        )
        .unwrap();
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::default();
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "ignored-client-claim");
    params.distinguished_name = distinguished_name;
    params.is_ca = IsCa::NoCa;
    let csr_pem = params.serialize_request(&key).unwrap().pem().unwrap();
    let issued = process
        .kernel_mut()
        .issue_device_certificate(
            &authority,
            &EnrollmentRequest {
                grant_id: grant.grant_id,
                grant_secret: grant.grant_secret,
                csr_pem,
            },
            now_ms,
        )
        .unwrap();
    let (_, leaf) = x509_parser::pem::parse_x509_pem(issued.certificate_pem.as_bytes()).unwrap();
    let (_, ca) = x509_parser::pem::parse_x509_pem(issued.ca_certificate_pem.as_bytes()).unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(ca.contents)).unwrap();
    let config = ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_root_certificates(roots)
        .with_client_auth_cert(
            vec![CertificateDer::from(leaf.contents)],
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key.serialize_der())),
        )
        .unwrap();
    EnrolledClient {
        config: Arc::new(config),
        handshake: HandshakeRequest {
            protocol_version: PROTOCOL_VERSION,
            device_id: issued.device_id,
            device_name: issued.device_name,
            role: issued.role,
            registry_revision: issued.registry_revision,
            capabilities,
        },
    }
}

async fn authenticated_request<T: Serialize + ?Sized>(
    endpoint: SocketAddr,
    client: &EnrolledClient,
    method: &str,
    path: &str,
    body: &T,
) -> String {
    let tcp = tokio::net::TcpStream::connect(endpoint).await.unwrap();
    let server_name = ServerName::IpAddress(endpoint.ip().into());
    let mut stream = TlsConnector::from(client.config.clone())
        .connect(server_name, tcp)
        .await
        .unwrap();
    let handshake = AuthenticatedHandshakeRequest {
        handshake: client.handshake.clone(),
        tls_exporter_sha256: exporter_digest(stream.get_ref().1),
    };
    let response = send_http_keep_alive(
        &mut stream,
        endpoint,
        "POST",
        "/v1/distributed/connections/accept",
        &serde_json::to_vec(&handshake).unwrap(),
    )
    .await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let _: HandshakeResponse = response_json(&response);
    send_http(
        &mut stream,
        endpoint,
        method,
        path,
        &serde_json::to_vec(body).unwrap(),
    )
    .await
}

async fn one_shot_request(
    endpoint: SocketAddr,
    config: Arc<ClientConfig>,
    method: &str,
    path: &str,
    body: &[u8],
) -> String {
    let tcp = tokio::net::TcpStream::connect(endpoint).await.unwrap();
    let server_name = ServerName::IpAddress(endpoint.ip().into());
    let mut stream = TlsConnector::from(config)
        .connect(server_name, tcp)
        .await
        .unwrap();
    send_http(&mut stream, endpoint, method, path, body).await
}

fn exporter_digest(connection: &rustls::ClientConnection) -> [u8; 32] {
    let mut exporter = [0_u8; 32];
    connection
        .export_keying_material(&mut exporter, TLS_EXPORTER_LABEL, None)
        .unwrap();
    Sha256::digest(exporter).into()
}

async fn send_http(
    stream: &mut tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    endpoint: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
) -> String {
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {endpoint}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
    stream.flush().await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

async fn send_http_keep_alive(
    stream: &mut tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    endpoint: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
) -> String {
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {endpoint}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
    stream.flush().await.unwrap();
    let mut response = Vec::new();
    while !response.ends_with(b"\r\n\r\n") {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).await.unwrap();
        response.push(byte[0]);
    }
    let headers = String::from_utf8_lossy(&response);
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    response.extend_from_slice(&body);
    String::from_utf8_lossy(&response).into_owned()
}

fn response_json<T: serde::de::DeserializeOwned>(response: &str) -> T {
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
}

fn spawn_server(
    binary: &str,
    data_dir: &Path,
    local_endpoint: SocketAddr,
    remote_endpoint: SocketAddr,
) -> ChildGuard {
    ChildGuard(
        Command::new(binary)
            .arg("--data-dir")
            .arg(data_dir)
            .arg("serve")
            .arg("--bind")
            .arg(local_endpoint.to_string())
            .arg("--remote-bind")
            .arg(remote_endpoint.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}

fn read_ready(child: &mut Child) {
    let stdout = child.stdout.take().unwrap();
    let mut line = String::new();
    BufReader::new(stdout).read_line(&mut line).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["status"], "ready");
}

fn unused_loopback_addr() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}
