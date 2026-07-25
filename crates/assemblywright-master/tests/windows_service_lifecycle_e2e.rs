#![cfg(windows)]

use serde_json::Value;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use uuid::Uuid;

struct InstalledServiceGuard {
    binary: &'static str,
    data_dir: PathBuf,
    service_name: String,
    installed: bool,
}

impl InstalledServiceGuard {
    fn uninstall(&mut self) -> Output {
        let output = run(
            self.binary,
            &self.data_dir,
            &[
                "service",
                "uninstall",
                "--service-name",
                &self.service_name,
                "--confirm",
            ],
        );
        if output.status.success() {
            self.installed = false;
        }
        output
    }
}

impl Drop for InstalledServiceGuard {
    fn drop(&mut self) {
        if self.installed {
            let _ = self.uninstall();
        }
    }
}

#[test]
#[ignore = "requires elevated Windows Service Control Manager access"]
fn windows_service_install_maintenance_recovery_and_uninstall_preserve_master_state() {
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    let directory = tempfile::tempdir().expect("service E2E data directory");
    let endpoint = unused_loopback_addr();
    let service_name = format!("AssemblywrightMasterE2E{}", Uuid::new_v4().simple());

    assert_success(
        &run(binary, directory.path(), &["setup"]),
        "initialize master data",
    );
    let install = run(
        binary,
        directory.path(),
        &[
            "service",
            "install",
            "--service-name",
            &service_name,
            "--bind",
            &endpoint.to_string(),
            "--identity",
            "local-system",
            "--confirm",
        ],
    );
    if !install.status.success()
        && String::from_utf8_lossy(&install.stderr).contains("Access is denied")
        && std::env::var("ASSEMBLYWRIGHT_REQUIRE_WINDOWS_SERVICE_E2E").as_deref() != Ok("1")
    {
        eprintln!(
            "skipping real SCM lifecycle because this Windows token is not elevated; CI requires it"
        );
        return;
    }
    assert_success(&install, "install Windows service");
    let mut guard = InstalledServiceGuard {
        binary,
        data_dir: directory.path().to_path_buf(),
        service_name: service_name.clone(),
        installed: true,
    };
    let install_receipt: Value = decode(&install);
    assert_eq!(install_receipt["status"], "service_installed");
    assert_eq!(install_receipt["start_type"], "automatic");
    assert_eq!(install_receipt["service_identity"], "LocalSystem");
    assert_eq!(install_receipt["service_logon_right"], "not_required");
    assert_eq!(
        install_receipt["recovery"]["restart_delays_seconds"],
        serde_json::json!([5, 15, 60])
    );
    assert_success(
        &run(
            binary,
            directory.path(),
            &["service", "start", "--service-name", &service_name],
        ),
        "start Windows service",
    );
    let status = run(
        binary,
        directory.path(),
        &[
            "service",
            "status",
            "--service-name",
            &service_name,
            "--endpoint",
            &endpoint.to_string(),
        ],
    );
    assert_success(&status, "inspect running Windows service");
    let status_receipt: Value = decode(&status);
    assert_eq!(status_receipt["service"]["scm_state"], "running");
    assert_eq!(status_receipt["runtime_health_available"], true);
    assert_eq!(
        status_receipt["runtime_health"]["host_mode"],
        "windows_service"
    );
    assert_eq!(
        status_receipt["runtime_health"]["service_identity"],
        "LocalSystem"
    );

    let maintenance = run(
        binary,
        directory.path(),
        &[
            "service",
            "maintenance-enter",
            "--service-name",
            &service_name,
            "--reason",
            "upgrade",
            "--confirm",
        ],
    );
    assert_success(&maintenance, "enter maintenance mode");
    let health = run(
        binary,
        directory.path(),
        &["health", "--endpoint", &endpoint.to_string()],
    );
    assert_success(&health, "read maintenance health");
    let health_receipt: Value = decode(&health);
    assert_eq!(health_receipt["status"], "maintenance");
    assert_eq!(health_receipt["maintenance_active"], true);
    assert_eq!(health_receipt["maintenance_reason"], "upgrade");

    let blocked = run(
        binary,
        directory.path(),
        &[
            "fixture-worker",
            "--endpoint",
            &endpoint.to_string(),
            "--prompt",
            "maintenance must block this new work",
        ],
    );
    assert!(!blocked.status.success(), "maintenance accepted new work");
    assert!(
        String::from_utf8_lossy(&blocked.stderr).contains("503 Service Unavailable"),
        "unexpected maintenance rejection: {}",
        String::from_utf8_lossy(&blocked.stderr)
    );

    let maintenance_recover = run(
        binary,
        directory.path(),
        &[
            "service",
            "recover",
            "--service-name",
            &service_name,
            "--endpoint",
            &endpoint.to_string(),
            "--confirm",
        ],
    );
    assert_success(
        &maintenance_recover,
        "recover Windows service during maintenance",
    );
    let maintenance_recover_receipt: Value = decode(&maintenance_recover);
    assert_eq!(maintenance_recover_receipt["status"], "service_recovered");
    assert_eq!(maintenance_recover_receipt["maintenance_preserved"], true);
    assert_eq!(
        maintenance_recover_receipt["runtime_health"]["status"],
        "maintenance"
    );
    assert_eq!(
        maintenance_recover_receipt["runtime_health"]["maintenance_reason"],
        "upgrade"
    );

    let blocked_after_restart = run(
        binary,
        directory.path(),
        &[
            "fixture-worker",
            "--endpoint",
            &endpoint.to_string(),
            "--prompt",
            "maintenance must survive service restart",
        ],
    );
    assert!(
        !blocked_after_restart.status.success(),
        "service restart cleared maintenance admission control"
    );
    assert!(
        String::from_utf8_lossy(&blocked_after_restart.stderr).contains("503 Service Unavailable"),
        "unexpected post-restart maintenance rejection: {}",
        String::from_utf8_lossy(&blocked_after_restart.stderr)
    );

    assert_success(
        &run(
            binary,
            directory.path(),
            &[
                "service",
                "maintenance-exit",
                "--service-name",
                &service_name,
                "--confirm",
            ],
        ),
        "exit maintenance mode",
    );
    assert_success(
        &run(
            binary,
            directory.path(),
            &[
                "fixture-worker",
                "--endpoint",
                &endpoint.to_string(),
                "--prompt",
                "work resumes after maintenance",
            ],
        ),
        "complete work after maintenance",
    );

    let stop = run(
        binary,
        directory.path(),
        &["service", "stop", "--service-name", &service_name],
    );
    assert_success(&stop, "stop Windows service");
    let stop_receipt: Value = decode(&stop);
    assert_eq!(stop_receipt["status"], "service_stopped");
    assert_eq!(stop_receipt["scm_state"], "stopped");

    let stopped_status = run(
        binary,
        directory.path(),
        &[
            "service",
            "status",
            "--service-name",
            &service_name,
            "--endpoint",
            &endpoint.to_string(),
        ],
    );
    assert_success(&stopped_status, "inspect stopped Windows service");
    let stopped_status_receipt: Value = decode(&stopped_status);
    assert_eq!(stopped_status_receipt["service"]["scm_state"], "stopped");
    assert_eq!(stopped_status_receipt["runtime_health_available"], false);

    let restart = run(
        binary,
        directory.path(),
        &["service", "start", "--service-name", &service_name],
    );
    assert_success(&restart, "restart Windows service after direct stop");
    let restart_receipt: Value = decode(&restart);
    assert_eq!(restart_receipt["status"], "service_started");
    assert_eq!(restart_receipt["scm_state"], "running");

    let restarted_status = run(
        binary,
        directory.path(),
        &[
            "service",
            "status",
            "--service-name",
            &service_name,
            "--endpoint",
            &endpoint.to_string(),
        ],
    );
    assert_success(&restarted_status, "inspect restarted Windows service");
    let restarted_status_receipt: Value = decode(&restarted_status);
    assert_eq!(restarted_status_receipt["service"]["scm_state"], "running");
    assert_eq!(restarted_status_receipt["runtime_health_available"], true);
    assert_eq!(restarted_status_receipt["runtime_health"]["status"], "ok");

    let uninstall = guard.uninstall();
    assert_success(&uninstall, "uninstall Windows service");
    let uninstall_receipt: Value = decode(&uninstall);
    assert_eq!(uninstall_receipt["master_data_preserved"], true);
    assert!(directory.path().join("master.sqlite3").is_file());
    assert!(directory.path().join("development.token").is_file());
}

fn run(binary: &str, data_dir: &Path, arguments: &[&str]) -> Output {
    Command::new(binary)
        .arg("--data-dir")
        .arg(data_dir)
        .args(arguments)
        .output()
        .expect("run assemblywright-master service command")
}

fn decode(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "decode command JSON: {error}; stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn unused_loopback_addr() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve loopback address")
        .local_addr()
        .expect("read loopback address")
}

fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
