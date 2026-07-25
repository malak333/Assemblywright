//! End-to-end coverage for the shipped release-readiness JSON boundary.
//!
//! The protocol implementation can be aligned while user-visible release
//! metadata remains stale. This test executes the real `assemblywright` binary,
//! parses its public JSON response, and verifies the exact feature proof an
//! owner or evidence collector receives.

use serde_json::Value;
use std::process::Command;

const CLI: &str = env!("CARGO_BIN_EXE_assemblywright");

fn normalized_tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| {
            token
                .trim_matches(|character: char| !character.is_ascii_alphanumeric())
                .to_ascii_lowercase()
        })
        .collect()
}

fn contains_numeric_protocol_version(text: &str) -> bool {
    normalized_tokens(text).windows(3).any(|tokens| {
        tokens[0] == "protocol" && tokens[1] == "version" && tokens[2].parse::<u16>().is_ok()
    })
}

#[test]
fn shipped_readiness_json_keeps_protocol_proof_version_independent() {
    let output = Command::new(CLI)
        .args(["release", "readiness", "--json"])
        .output()
        .expect("run the shipped Assemblywright CLI");
    assert!(
        output.status.success(),
        "release readiness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("decode release readiness JSON");
    let protocol_features = response["implemented_features"]
        .as_array()
        .expect("implemented_features is an array")
        .iter()
        .filter(|feature| feature["key"] == "distributed_protocol_contract")
        .collect::<Vec<_>>();
    assert_eq!(
        protocol_features.len(),
        1,
        "readiness must expose exactly one distributed protocol feature"
    );

    let feature = protocol_features[0];
    assert_eq!(feature["status"], "implemented");
    let proof = feature["proof"]
        .as_str()
        .expect("distributed protocol proof is a string");
    assert!(
        proof.contains("the current protocol version"),
        "readiness must describe the live declaration without duplicating its number: {proof}"
    );
    assert!(
        !contains_numeric_protocol_version(proof),
        "readiness must not ship a numeric protocol version that can drift: {proof}"
    );
}
