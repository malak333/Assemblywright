#![cfg(unix)]

use assemblywright_protocol::{
    FeatureConveyorGrantRevisions, FeatureConveyorKnowledgeBaseDetermination,
    FeatureConveyorReviewCoverageStatus, FeatureConveyorReviewDecision,
    FeatureConveyorReviewPacket, FeatureConveyorReviewProviderOutput,
    FeatureConveyorReviewRequirementCoverage, FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use tempfile::tempdir;
use uuid::Uuid;

fn packet() -> FeatureConveyorReviewPacket {
    let approved_specification = json!({
        "acceptance": ["adapter-requirement"],
        "outcome": "prove the response-only adapter"
    });
    let approved_specification_sha256 =
        Sha256::digest(serde_json::to_vec(&approved_specification).unwrap()).into();
    let candidate_diff = "diff --git a/proof.txt b/proof.txt\n".to_string();
    FeatureConveyorReviewPacket {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        feature_id: Uuid::from_u128(1),
        specification_revision: 1,
        approved_specification,
        approved_specification_sha256,
        candidate_commit: "1".repeat(40),
        candidate_tree: "2".repeat(40),
        base_commit: "3".repeat(40),
        candidate_diff_sha256: Sha256::digest(candidate_diff.as_bytes()).into(),
        candidate_diff,
        evidence_manifest_sha256: [4; 32],
        evidence_digests: vec![[4; 32], [5; 32]],
        requirements_sha256: [6; 32],
        requirement_ids: vec!["adapter-requirement".to_string()],
        provider_id: "openai.codex".to_string(),
        model_id: "gpt-5.6-sol".to_string(),
        grants: FeatureConveyorGrantRevisions {
            registration: 1,
            cloud_disclosure: 1,
            autonomous_publication: 1,
        },
    }
}

#[test]
fn adapter_uses_only_fixed_environment_and_disabled_tool_configuration() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    let codex_home = root.join("codex-home");
    fs::create_dir(&codex_home).unwrap();
    fs::write(codex_home.join("auth.json"), b"fixture").unwrap();
    let schema = root.join("review-output-schema.json");
    fs::write(&schema, b"{}").unwrap();

    let packet = packet();
    let output = FeatureConveyorReviewProviderOutput {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_packet_sha256: packet.sha256().unwrap(),
        provider_id: packet.provider_id.clone(),
        model_id: packet.model_id.clone(),
        decision: FeatureConveyorReviewDecision::Approved,
        blocking_findings: vec![],
        non_blocking_findings: vec![],
        requirement_coverage: vec![FeatureConveyorReviewRequirementCoverage {
            requirement_id: packet.requirement_ids[0].clone(),
            status: FeatureConveyorReviewCoverageStatus::Covered,
            evidence_sha256: packet.evidence_digests[0],
        }],
        evidence_digests: packet.evidence_digests.clone(),
        knowledge_base_determination: FeatureConveyorKnowledgeBaseDetermination::NoNewKnowledge,
        knowledge_base_evidence_sha256: packet.evidence_digests[1],
    };
    let output_json = serde_json::to_string(&output).unwrap();
    let codex = root.join("codex");
    fs::write(
        &codex,
        format!(
            "#!/bin/sh\nset -eu\n[ \"${{CODEX_HOME-}}\" = '{}' ]\nargs=\" $* \"\ncase \"$args\" in *' features.shell_tool=false '*) ;; *) exit 21;; esac\ncase \"$args\" in *' web_search=\"disabled\" '*) ;; *) exit 22;; esac\ncase \"$args\" in *' tools.web_search=false '*) ;; *) exit 23;; esac\ncase \"$args\" in *' --strict-config '*) ;; *) exit 24;; esac\ncase \"$args\" in *' features.view_image=false '*) ;; *) exit 25;; esac\ncase \"$args\" in *' features.image_generation=false '*) ;; *) exit 26;; esac\ncase \"$args\" in *' features.skill_mcp_dependency_install=false '*) ;; *) exit 27;; esac\ncase \"$args\" in *' features.skill_search=false '*) ;; *) exit 28;; esac\ncase \"$args\" in *' features.plugins=false '*) ;; *) exit 29;; esac\ncat >/dev/null\nprintf '%s' '{}'
",
            codex_home.display(),
            output_json.replace('\'', "'\\''")
        ),
    )
    .unwrap();
    fs::set_permissions(&codex, fs::Permissions::from_mode(0o700)).unwrap();

    let canonical = packet.canonical_bytes().unwrap();
    let adapter = env!("CARGO_BIN_EXE_assemblywright-review-provider");
    let configure = |command: &mut Command| {
        command
            .env_clear()
            .env("ASSEMBLYWRIGHT_REVIEW_CODEX_HOME", &codex_home)
            .env("ASSEMBLYWRIGHT_REVIEW_CODEX_EXECUTABLE", &codex)
            .env("ASSEMBLYWRIGHT_REVIEW_OUTPUT_SCHEMA", &schema)
            .env("ASSEMBLYWRIGHT_REVIEW_MODEL_ID", "gpt-5.6-sol")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    };

    let mut count = Command::new(adapter);
    configure(&mut count);
    count.arg("--count-tokens");
    let mut count = count.spawn().unwrap();
    count.stdin.take().unwrap().write_all(&canonical).unwrap();
    let count = count.wait_with_output().unwrap();
    assert!(count.status.success(), "{:?}", count.stderr);
    assert_eq!(
        String::from_utf8(count.stdout).unwrap(),
        canonical.len().to_string()
    );

    let mut review = Command::new(adapter);
    configure(&mut review);
    let mut review = review.spawn().unwrap();
    review.stdin.take().unwrap().write_all(&canonical).unwrap();
    let review = review.wait_with_output().unwrap();
    assert!(review.status.success(), "{:?}", review.stderr);
    assert_eq!(
        FeatureConveyorReviewProviderOutput::decode_frame(&review.stdout).unwrap(),
        output
    );

    let mut polluted = Command::new(adapter);
    configure(&mut polluted);
    polluted.env("UNEXPECTED", "rejected");
    let mut polluted = polluted.spawn().unwrap();
    polluted
        .stdin
        .take()
        .unwrap()
        .write_all(&canonical)
        .unwrap();
    assert!(!polluted.wait().unwrap().success());
}
