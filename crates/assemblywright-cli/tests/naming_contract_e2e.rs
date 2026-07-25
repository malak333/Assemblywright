//! End-to-end coverage for the shipped CLI's name and the commands it emits.
//!
//! The readiness and runbook payloads are *generated strings*, so no manifest or
//! path check can see them. A rename pass once left a former package name in
//! them, which meant following the CLI's own runbook invoked a package that did
//! not exist. These tests execute the real binary under its shipped name and
//! assert that every command it tells an owner to run still resolves.
//!
//! The check is deliberately positive: rather than blacklisting names that used
//! to exist, it reads the workspace members and requires every emitted
//! `cargo run -p <package>` to name one of them. That catches any stale package
//! reference, not just the ones someone remembered to list.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolving this env var at compile time only succeeds while the binary target
/// is named `assemblywright`, so renaming the binary fails the build here.
const CLI: &str = env!("CARGO_BIN_EXE_assemblywright");

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("resolve the workspace root from the crate manifest directory")
        .to_path_buf()
}

/// Package names declared by the workspace, derived from the member paths so the
/// test cannot drift from `Cargo.toml`.
fn workspace_packages() -> BTreeSet<String> {
    let manifest = std::fs::read_to_string(workspace_root().join("Cargo.toml"))
        .expect("read the workspace manifest");
    let members = manifest
        .split_once("members = [")
        .expect("workspace manifest declares members")
        .1
        .split_once(']')
        .expect("workspace members list is closed")
        .0;
    let packages: BTreeSet<String> = members
        .split(',')
        .filter_map(|entry| {
            let entry = entry.trim().trim_matches('"');
            entry
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
        .collect();
    assert!(
        packages.contains("assemblywright-cli"),
        "expected the workspace to declare this crate, got {packages:?}"
    );
    packages
}

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

const RELEASE_SUBCOMMANDS: [&str; 5] = [
    "readiness",
    "evidence-status",
    "signed-distribution-runbook",
    "live-device-runbook",
    "evidence-bundle-runbook",
];

#[test]
fn the_shipped_binary_is_named_assemblywright() {
    assert_eq!(
        Path::new(CLI).file_name().and_then(|name| name.to_str()),
        Some("assemblywright"),
        "the release artifact and every runbook reference this filename"
    );
    assert!(run(&["--version"]).starts_with("assemblywright "));
}

#[test]
fn every_emitted_cargo_command_names_a_real_workspace_package() {
    let packages = workspace_packages();
    let mut checked = 0usize;

    for subcommand in RELEASE_SUBCOMMANDS {
        for output in [
            run(&["release", subcommand]),
            run(&["release", subcommand, "--json"]),
        ] {
            for fragment in output.split("cargo run -p ").skip(1) {
                let emitted = fragment
                    .split_whitespace()
                    .next()
                    .expect("a package name follows `cargo run -p`");
                assert!(
                    packages.contains(emitted),
                    "release {subcommand} emits `cargo run -p {emitted}`, which is not a \
                     workspace package; members are {packages:?}"
                );
                checked += 1;
            }
        }
    }

    assert!(
        checked > 0,
        "expected the release runbooks to emit at least one cargo command"
    );
}

/// The readiness runbook is the owner's entry point, so it has to hand over a
/// command that actually runs, and it must name the bundled CLI filename that
/// signed provenance and live-device QA reports bind.
#[test]
fn readiness_hands_over_runnable_commands_and_names_the_bundled_cli() {
    let readiness = run(&["release", "readiness", "--all-commands"]);
    assert!(
        readiness.contains("cargo run -p assemblywright-cli --"),
        "readiness must hand the owner a runnable command"
    );

    let evidence = run(&["release", "evidence-status"]);
    assert!(
        evidence.contains("assemblywright-cli") || readiness.contains("assemblywright-cli"),
        "the bundled CLI inside Assemblywright.app is `assemblywright-cli`; signed \
         provenance and live-device QA reports bind that filename"
    );
}
