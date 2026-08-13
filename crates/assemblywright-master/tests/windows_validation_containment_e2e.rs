#![cfg(windows)]

use assemblywright_master::validation_containment::{
    run_validation_fixture, ValidationFixtureCommand,
};
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenIsAppContainer, TokenPrivileges, SE_PRIVILEGE_ENABLED, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

#[test]
fn appcontainer_fixture_uses_only_granted_root_exact_environment_and_bounded_output() {
    let root = tempfile::tempdir().expect("isolated execution root");
    let result = run_validation_fixture(
        ValidationFixtureCommand::ReadWriteAndEnvironment,
        root.path(),
        Duration::from_secs(10),
    )
    .expect("contained fixture");
    assert!(!result.timed_out);
    assert_eq!(
        result.exit_code,
        0,
        "contained fixture failed: {}",
        std::fs::read_to_string(root.path().join("containment-result.txt"))
            .unwrap_or_else(|_| "missing containment result".to_string())
    );
    assert!(result.stdout_len <= 64 * 1024);
    assert!(root.path().join("fixture-output.txt").is_file());
}

#[test]
fn timeout_terminates_and_reaps_the_entire_fixture_job_tree() {
    let root = tempfile::tempdir().expect("isolated execution root");
    let result = run_validation_fixture(
        ValidationFixtureCommand::TimeoutChildTree,
        root.path(),
        Duration::from_millis(500),
    )
    .expect("timed out fixture is reaped");
    assert!(
        result.timed_out,
        "contained child did not reach timeout: exit_code={}; diagnostic={}",
        result.exit_code,
        std::fs::read_to_string(root.path().join("descendant-spawn-result.txt"))
            .unwrap_or_else(|_| "missing".to_string())
    );
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        !root.path().join("descendant-survived.txt").exists(),
        "a descendant survived job-wide timeout termination"
    );
}

#[test]
fn oversized_fixture_output_is_rejected_instead_of_becoming_evidence() {
    let root = tempfile::tempdir().expect("isolated execution root");
    let result = run_validation_fixture(
        ValidationFixtureCommand::BoundedOutput,
        root.path(),
        Duration::from_secs(10),
    );
    assert!(result.is_err());
}

#[test]
#[ignore = "hostile boundary proof; run on the owner-controlled Windows validation host"]
fn appcontainer_sid_cannot_read_or_write_outside_the_granted_execution_root() {
    let root = tempfile::tempdir().expect("isolated execution root");
    let outside = tempfile::tempdir().expect("outside root");
    let protected = outside.path().join("master-token-fixture");
    std::fs::write(&protected, b"owner-only").expect("outside fixture");
    std::fs::write(
        root.path().join("outside-probe.txt"),
        protected.to_string_lossy().as_bytes(),
    )
    .expect("outside probe path");
    let result = run_validation_fixture(
        ValidationFixtureCommand::DeniedOutsideRoot,
        root.path(),
        Duration::from_secs(10),
    )
    .expect("hostile fixture");
    assert_eq!(
        result.exit_code, 0,
        "fixture reports success only after access denial"
    );
    assert_eq!(
        std::fs::read(protected).expect("outside fixture remains readable to owner"),
        b"owner-only"
    );
}

#[test]
#[ignore = "hostile boundary proof; run on the owner-controlled Windows validation host"]
fn zero_capability_appcontainer_cannot_open_a_loopback_network_connection() {
    let root = tempfile::tempdir().expect("isolated execution root");
    let listener = TcpListener::bind("127.0.0.1:0").expect("probe listener");
    let udp = UdpSocket::bind("127.0.0.1:0").expect("udp probe listener");
    udp.set_read_timeout(Some(Duration::from_millis(250)))
        .expect("udp read timeout");
    std::fs::write(
        root.path().join("network-probe.txt"),
        format!(
            "{}\n{}\n",
            listener.local_addr().expect("probe address"),
            udp.local_addr().expect("udp probe address")
        ),
    )
    .expect("probe file");
    let result = run_validation_fixture(
        ValidationFixtureCommand::NetworkDenied,
        root.path(),
        Duration::from_secs(10),
    )
    .expect("hostile fixture");
    assert_eq!(
        result.exit_code,
        0,
        "fixture reports success only after network denial: {}",
        std::fs::read_to_string(root.path().join("network-result.txt"))
            .unwrap_or_else(|_| "missing network result".to_string())
    );
    let mut datagram = [0u8; 16];
    assert!(
        udp.recv_from(&mut datagram).is_err(),
        "zero-capability AppContainer delivered a loopback UDP datagram"
    );
    drop(listener);
    drop(udp);
}

#[test]
#[ignore = "child fixture invoked only through the contained runner"]
fn fixture_appcontainer_can_read_write_only_granted_root_and_has_exact_environment() {
    let mut token: HANDLE = std::ptr::null_mut();
    let token_opened =
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0;
    let mut is_appcontainer = 0u32;
    let mut returned = 0u32;
    let token_queried = token_opened
        && unsafe {
            GetTokenInformation(
                token,
                TokenIsAppContainer,
                (&mut is_appcontainer as *mut u32).cast(),
                std::mem::size_of::<u32>() as u32,
                &mut returned,
            )
        } != 0;
    let privileges = token_opened.then(|| token_privilege_counts(token));
    if token_opened {
        unsafe { CloseHandle(token) };
    }

    assert!(
        std::fs::read_to_string("inheritance-probe.txt")
            .expect("inheritance probe")
            .parse::<usize>()
            .is_ok(),
        "inheritance probe is malformed"
    );
    let keys = std::env::vars_os()
        .map(|(key, _)| key.to_string_lossy().to_ascii_uppercase())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        keys,
        [
            "APPDATA",
            "COMSPEC",
            "LOCALAPPDATA",
            "PATH",
            "SYSTEMDRIVE",
            "SYSTEMROOT",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "WINDIR",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        "child environment was not the exact protocol-owned minimum"
    );
    std::fs::write(
        "containment-result.txt",
        format!(
            "token_opened={token_opened}; token_queried={token_queried}; appcontainer={}; privileges={privileges:?}",
            is_appcontainer == 1,
        ),
    )
    .expect("containment result");
    assert!(token_opened, "child process token could not be opened");
    assert!(token_queried, "child process token could not be queried");
    assert_eq!(is_appcontainer, 1, "child token is not an AppContainer");
    let (privilege_count, enabled_privilege_count) = privileges
        .expect("child token was not opened")
        .expect("child token privileges could not be queried");
    assert!(
        privilege_count <= 1 && enabled_privilege_count <= 1,
        "child retained more than the single Windows traversal privilege"
    );
    std::fs::write("fixture-output.txt", b"contained\n").expect("write granted root");
    assert_eq!(
        std::fs::read("fixture-output.txt").expect("read granted root"),
        b"contained\n"
    );
}

fn token_privilege_counts(token: HANDLE) -> Result<(u32, u32), u32> {
    let mut required = 0u32;
    unsafe {
        GetTokenInformation(
            token,
            TokenPrivileges,
            std::ptr::null_mut(),
            0,
            &mut required,
        )
    };
    if required < std::mem::size_of::<u32>() as u32 {
        return Err(required);
    }
    let words = required.div_ceil(std::mem::size_of::<usize>() as u32) as usize;
    let mut buffer = vec![0usize; words];
    if unsafe {
        GetTokenInformation(
            token,
            TokenPrivileges,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(required);
    }
    let bytes = buffer.as_ptr().cast::<u8>();
    let count = unsafe { std::ptr::read_unaligned(bytes.cast::<u32>()) };
    let expected = 4usize.saturating_add(count as usize * 12);
    if expected > required as usize {
        return Err(required);
    }
    let enabled = (0..count)
        .filter(|index| {
            let attributes = unsafe {
                std::ptr::read_unaligned(bytes.add(4 + *index as usize * 12 + 8).cast::<u32>())
            };
            attributes & SE_PRIVILEGE_ENABLED != 0
        })
        .count() as u32;
    Ok((count, enabled))
}

#[test]
#[ignore = "child fixture invoked only through the contained runner"]
fn fixture_appcontainer_output_is_bounded_by_parent() {
    let mut stdout = std::io::stdout().lock();
    let block = [b'x'; 4096];
    for _ in 0..32 {
        stdout.write_all(&block).expect("fixture stdout");
    }
}

#[test]
#[ignore = "child fixture invoked only through the contained runner"]
fn fixture_timeout_spawns_child_tree_that_must_be_killed() {
    let child = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "fixture_timeout_descendant_never_finishes",
            "--ignored",
            "--nocapture",
        ])
        .spawn();
    std::fs::write(
        "descendant-spawn-result.txt",
        match &child {
            Ok(child) => format!("spawned={}", child.id()),
            Err(error) => format!(
                "denied_kind={:?};raw={:?}",
                error.kind(),
                error.raw_os_error()
            ),
        },
    )
    .expect("record descendant spawn result");
    let child = child.expect("spawn descendant");
    std::fs::write("descendant.pid", child.id().to_string()).expect("record descendant pid");
    std::thread::sleep(Duration::from_secs(60));
}

#[test]
#[ignore = "child fixture invoked only through the contained runner"]
fn fixture_timeout_descendant_never_finishes() {
    std::thread::sleep(Duration::from_secs(2));
    std::fs::write("descendant-survived.txt", b"job containment failed")
        .expect("write survival marker");
}

#[test]
#[ignore = "child fixture invoked only through the contained runner"]
fn fixture_appcontainer_is_denied_outside_execution_root() {
    let outside =
        PathBuf::from(std::fs::read_to_string("outside-probe.txt").expect("outside probe path"));
    assert!(
        std::fs::read(&outside).is_err(),
        "AppContainer read protected host state"
    );
    assert!(
        std::fs::write(&outside, b"denied").is_err(),
        "AppContainer wrote outside execution root"
    );
}

#[test]
#[ignore = "child fixture invoked only through the contained runner"]
fn fixture_zero_capability_appcontainer_has_no_network() {
    let probes = std::fs::read_to_string("network-probe.txt").expect("read probe addresses");
    let mut probes = probes.lines();
    let tcp: SocketAddr = probes
        .next()
        .expect("tcp address")
        .parse()
        .expect("tcp address");
    let udp: SocketAddr = probes
        .next()
        .expect("udp address")
        .parse()
        .expect("udp address");
    let tcp_denied = TcpStream::connect_timeout(&tcp, Duration::from_millis(500)).is_err();
    let udp_send_succeeded = match UdpSocket::bind("127.0.0.1:0") {
        Err(_) => false,
        Ok(socket) => socket.send_to(b"denied", udp).is_ok(),
    };
    std::fs::write(
        "network-result.txt",
        format!("tcp_denied={tcp_denied};udp_send_succeeded={udp_send_succeeded}"),
    )
    .expect("write network result");
    assert!(tcp_denied);
}
