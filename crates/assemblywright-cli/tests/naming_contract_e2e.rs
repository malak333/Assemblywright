//! End-to-end coverage for the shipped CLI's name and the commands it emits.
//!
//! The Assemblywright rename changed the binary from `jarvis` to
//! `assemblywright`, but the readiness and runbook payloads are *generated
//! strings* that no manifest check can see. The first rename pass left
//! `cargo run -p jarvis-cli` in them, so following the CLI's own runbook invoked
//! a package that no longer exists. These tests execute the real binary under
//! its shipped name and assert that every command it tells an owner to run is a
//! command that still resolves.

use std::path::Path;
use std::process::Command;

/// Resolving this env var at compile time only succeeds while the binary target
/// is named `assemblywright`. A rename back to `jarvis` fails to build.
const CLI: &str = env!("CARGO_BIN_EXE_assemblywright");

fn run(args: &[&str]) -> String {
    let output = Command::new(CLI)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run assemblywright {args:?}: {error}"));
    assert!(
        output.status.success(),
        "assemblywright {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 CLI output")
}

#[test]
fn the_shipped_binary_is_named_assemblywright() {
    assert_eq!(
        Path::new(CLI).file_name().and_then(|name| name.to_str()),
        Some("assemblywright"),
        "the release artifact and every runbook reference this filename"
    );
    assert!(run(&["--version"]).starts_with("assemblywright "));
}

/// Every `cargo run -p <package>` command the CLI emits must name a package that
/// exists in the workspace. `jarvis-cli` and friends are gone, so emitting them
/// hands the owner a command that cannot run.
#[test]
fn emitted_runbook_commands_reference_only_live_workspace_packages() {
    const REMOVED_PACKAGES: [&str; 5] = [
        "-p jarvis-cli",
        "-p jarvis-core",
        "-p jarvis-agent",
        "-p jarvis-master",
        "-p jarvis-protocol",
    ];

    for subcommand in [
        "readiness",
        "evidence-status",
        "signed-distribution-runbook",
        "live-device-runbook",
        "evidence-bundle-runbook",
    ] {
        for output in [
            run(&["release", subcommand]),
            run(&["release", subcommand, "--json"]),
        ] {
            for removed in REMOVED_PACKAGES {
                assert!(
                    !output.contains(removed),
                    "release {subcommand} emits a removed package: {removed}"
                );
            }
            for legacy_crate in ["`jarvis-protocol`", "`jarvis-master`", "`jarvis-core`"] {
                assert!(
                    !output.contains(legacy_crate),
                    "release {subcommand} describes a removed crate: {legacy_crate}"
                );
            }
        }
    }
}

/// The readiness runbook is the owner's entry point, so it must point at the
/// current package and keep naming the preserved bundled-CLI filename, which is
/// a signed-artifact contract rather than drift.
#[test]
fn readiness_points_at_the_current_package_and_keeps_signed_identity_names() {
    let readiness = run(&["release", "readiness", "--all-commands"]);
    assert!(
        readiness.contains("cargo run -p assemblywright-cli --"),
        "readiness must hand the owner a runnable command"
    );

    let evidence = run(&["release", "evidence-status"]);
    assert!(
        evidence.contains("jarvis-cli") || readiness.contains("jarvis-cli"),
        "the bundled CLI filename inside Assemblywright.app stays `jarvis-cli`; \
         signed provenance and live-device QA reports bind it"
    );
}
