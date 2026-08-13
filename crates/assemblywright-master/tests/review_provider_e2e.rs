use assemblywright_master::{
    invoke_review_provider, prepare_review_provider_call, ProcessReviewProvider, ReviewProvider,
    ReviewProviderCapabilities, ReviewProviderInvocationError, ReviewProviderTokenCountError,
    ReviewProviderTransportError, UnavailableReviewProvider,
    MAX_FEATURE_CONVEYOR_REVIEW_INPUT_TOKENS,
};
use assemblywright_protocol::{
    FeatureConveyorGrantRevisions, FeatureConveyorKnowledgeBaseDetermination,
    FeatureConveyorReviewCoverageStatus, FeatureConveyorReviewDecision,
    FeatureConveyorReviewGatewayRequest, FeatureConveyorReviewPacket,
    FeatureConveyorReviewProviderOutput, FeatureConveyorReviewRequirementCoverage,
    FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION, MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES,
    MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES,
};
use serde_json::json;
use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(any(unix, windows))]
use std::time::{Duration, Instant};
#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};
#[cfg(any(unix, windows))]
use tempfile::tempdir;
use uuid::Uuid;

fn packet_and_request() -> (
    FeatureConveyorReviewPacket,
    FeatureConveyorReviewGatewayRequest,
) {
    let approved_specification = json!({"outcome":"native fake provider e2e"});
    let approved_specification_sha256 =
        Sha256::digest(b"{\"outcome\":\"native fake provider e2e\"}").into();
    let candidate_diff = "diff --git a/README.md b/README.md\n".to_string();
    let packet = FeatureConveyorReviewPacket {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        feature_id: Uuid::from_u128(2),
        specification_revision: 3,
        approved_specification,
        approved_specification_sha256,
        candidate_commit: "1111111111111111111111111111111111111111".to_string(),
        candidate_tree: "2222222222222222222222222222222222222222".to_string(),
        base_commit: "3333333333333333333333333333333333333333".to_string(),
        candidate_diff_sha256: Sha256::digest(candidate_diff.as_bytes()).into(),
        candidate_diff,
        evidence_manifest_sha256: [4; 32],
        evidence_digests: vec![[4; 32], [5; 32]],
        requirements_sha256: [6; 32],
        requirement_ids: vec!["acceptance-criterion-0001".to_string()],
        provider_id: "fake.review".to_string(),
        model_id: "fake-v1".to_string(),
        grants: FeatureConveyorGrantRevisions {
            registration: 7,
            cloud_disclosure: 8,
            autonomous_publication: 9,
        },
    };
    let request = FeatureConveyorReviewGatewayRequest {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_call_id: Uuid::from_u128(10),
        feature_id: packet.feature_id,
        specification_revision: packet.specification_revision,
        expected_lifecycle_revision: 11,
        feature_lease_id: Uuid::from_u128(12),
        integration_id: Uuid::from_u128(13),
        validation_id: Uuid::from_u128(14),
        candidate_commit: packet.candidate_commit.clone(),
        candidate_tree: packet.candidate_tree.clone(),
        base_commit: packet.base_commit.clone(),
        candidate_diff_sha256: packet.candidate_diff_sha256,
        evidence_manifest_sha256: packet.evidence_manifest_sha256,
        review_packet_sha256: packet.sha256().unwrap(),
        provider_id: packet.provider_id.clone(),
        model_id: packet.model_id.clone(),
        expected_queue_revision: 15,
        expected_emergency_pause_revision: 16,
        grants: packet.grants,
    };
    (packet, request)
}

struct FakeProvider {
    calls: AtomicUsize,
    malformed: bool,
}

impl ReviewProvider for FakeProvider {
    fn capabilities(&self) -> Option<ReviewProviderCapabilities> {
        Some(ReviewProviderCapabilities {
            provider_id: "fake.review".to_string(),
            model_id: "fake-v1".to_string(),
            max_input_bytes: MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES,
            max_input_tokens: MAX_FEATURE_CONVEYOR_REVIEW_INPUT_TOKENS,
            max_output_bytes: MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES,
            strict_structured_output: true,
            response_only: true,
            fresh_session_per_call: true,
        })
    }

    fn count_input_tokens(
        &self,
        canonical_packet: &[u8],
    ) -> Result<u64, ReviewProviderTokenCountError> {
        Ok(u64::try_from(canonical_packet.len() / 4 + 1).unwrap())
    }

    fn review_response_only(
        &self,
        request: &FeatureConveyorReviewGatewayRequest,
        canonical_packet: &[u8],
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, ReviewProviderTransportError> {
        assert!(!cancelled.load(Ordering::Acquire));
        self.calls.fetch_add(1, Ordering::AcqRel);
        if self.malformed {
            return Ok(br#"{"decision":"approved","transcript":"forbidden"}"#.to_vec());
        }
        let packet: FeatureConveyorReviewPacket = serde_json::from_slice(canonical_packet).unwrap();
        assert_eq!(packet.sha256().unwrap(), request.review_packet_sha256);
        serde_json::to_vec(&FeatureConveyorReviewProviderOutput {
            schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
            review_packet_sha256: request.review_packet_sha256,
            provider_id: request.provider_id.clone(),
            model_id: request.model_id.clone(),
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
            knowledge_base_evidence_sha256: packet.evidence_digests[0],
        })
        .map_err(|_| ReviewProviderTransportError::IncompleteTransport)
    }
}

#[test]
fn fake_native_provider_receives_one_fresh_response_only_packet() {
    let (packet, request) = packet_and_request();
    let provider = FakeProvider {
        calls: AtomicUsize::new(0),
        malformed: false,
    };
    let prepared = prepare_review_provider_call(&provider, &request, &packet).unwrap();
    assert_eq!(provider.calls.load(Ordering::Acquire), 0);
    let output =
        invoke_review_provider(&provider, &request, &prepared, &AtomicBool::new(false)).unwrap();
    assert_eq!(provider.calls.load(Ordering::Acquire), 1);
    assert_eq!(output.decision, FeatureConveyorReviewDecision::Approved);
    assert_eq!(output.review_packet_sha256, request.review_packet_sha256);
}

#[test]
fn production_default_is_unavailable_before_any_provider_call() {
    let (packet, request) = packet_and_request();
    assert_eq!(
        prepare_review_provider_call(&UnavailableReviewProvider, &request, &packet).unwrap_err(),
        ReviewProviderInvocationError::Unavailable
    );
}

#[test]
fn malformed_output_and_precall_cancellation_fail_closed() {
    let (packet, request) = packet_and_request();
    let provider = FakeProvider {
        calls: AtomicUsize::new(0),
        malformed: true,
    };
    let prepared = prepare_review_provider_call(&provider, &request, &packet).unwrap();
    assert_eq!(
        invoke_review_provider(&provider, &request, &prepared, &AtomicBool::new(false))
            .unwrap_err(),
        ReviewProviderInvocationError::MalformedOutput
    );
    let cancelled = AtomicBool::new(true);
    assert_eq!(
        invoke_review_provider(&provider, &request, &prepared, &cancelled).unwrap_err(),
        ReviewProviderInvocationError::Cancelled
    );
    assert_eq!(provider.calls.load(Ordering::Acquire), 1);
}

#[cfg(unix)]
#[test]
fn configured_adapter_uses_a_fresh_cleared_environment_process_per_call() {
    let (packet, request) = packet_and_request();
    let output = FeatureConveyorReviewProviderOutput {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_packet_sha256: request.review_packet_sha256,
        provider_id: request.provider_id.clone(),
        model_id: request.model_id.clone(),
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
        knowledge_base_evidence_sha256: packet.evidence_digests[0],
    };
    let directory = tempdir().unwrap();
    let root = directory.path().join("review-provider");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        root.join("provider.json"),
        br#"{"schema_version":1,"provider_id":"fake.review","model_id":"fake-v1","max_input_tokens":64000}"#,
    )
    .unwrap();
    fs::set_permissions(
        root.join("provider.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let executable = root.join("review-provider");
    let output_json = serde_json::to_string(&output).unwrap();
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nset -eu\n[ -z \"${{HOME+x}}\" ] || exit 9\nif [ \"${{1-}}\" = \"--count-tokens\" ]; then cat >/dev/null; printf 100; exit 0; fi\ncat >/dev/null\nsleep 30 &\nprintf '%s' \"$!\" > descendant.pid\nprintf x >> invocations\nprintf '%s' '{output_json}'\n"
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&executable, permissions).unwrap();

    let provider = ProcessReviewProvider::load(directory.path())
        .unwrap()
        .unwrap();
    let prepared = prepare_review_provider_call(&provider, &request, &packet).unwrap();
    let started = Instant::now();
    for _ in 0..2 {
        let decision =
            invoke_review_provider(&provider, &request, &prepared, &AtomicBool::new(false))
                .unwrap();
        assert_eq!(decision.decision, FeatureConveyorReviewDecision::Approved);
    }
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(fs::read(root.join("invocations")).unwrap(), b"xx");
    let descendant: i32 = fs::read_to_string(root.join("descendant.pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(unsafe { libc::kill(descendant, 0) }, -1);

    fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
    assert_eq!(
        prepare_review_provider_call(&provider, &request, &packet).unwrap_err(),
        ReviewProviderInvocationError::Unavailable
    );
}

#[cfg(windows)]
#[test]
fn configured_windows_adapter_gates_spawn_inside_job_and_locks_verified_image() {
    use std::process::Command;
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    let (packet, request) = packet_and_request();
    let output = FeatureConveyorReviewProviderOutput {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_packet_sha256: request.review_packet_sha256,
        provider_id: request.provider_id.clone(),
        model_id: request.model_id.clone(),
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
        knowledge_base_evidence_sha256: packet.evidence_digests[0],
    };
    let directory = tempdir().unwrap();
    let root = directory.path().join("review-provider");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("provider.json"),
        br#"{"schema_version":1,"provider_id":"fake.review","model_id":"fake-v1","max_input_tokens":64000}"#,
    )
    .unwrap();
    let executable = root.join("review-provider.exe");
    let source = root.join("provider_fixture.rs");
    let output_literal = format!("{:?}", serde_json::to_string(&output).unwrap());
    fs::write(
        &source,
        format!(
            r#"use std::fs::OpenOptions;
use std::io::{{Read, Write}};
use std::process::Command;
use std::time::Duration;
fn main() {{
    let first = std::env::args().nth(1);
    if first.as_deref() == Some("--descendant") {{
        std::thread::sleep(Duration::from_secs(30));
        return;
    }}
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    if first.as_deref() == Some("--count-tokens") {{
        print!("100");
        return;
    }}
    let child = Command::new(std::env::current_exe().unwrap())
        .arg("--descendant")
        .spawn()
        .unwrap();
    std::fs::write("descendant.pid", child.id().to_string()).unwrap();
    OpenOptions::new().create(true).append(true).open("invocations")
        .unwrap().write_all(b"x").unwrap();
    print!("{{}}", {output_literal});
}}
"#
        ),
    )
    .unwrap();
    assert!(Command::new("rustc")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .status()
        .unwrap()
        .success());

    let provider = ProcessReviewProvider::load(directory.path())
        .unwrap()
        .unwrap();
    let prepared = prepare_review_provider_call(&provider, &request, &packet).unwrap();
    let started = Instant::now();
    for _ in 0..2 {
        let decision =
            invoke_review_provider(&provider, &request, &prepared, &AtomicBool::new(false))
                .unwrap();
        assert_eq!(decision.decision, FeatureConveyorReviewDecision::Approved);
    }
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(fs::read(root.join("invocations")).unwrap(), b"xx");
    let descendant: u32 = fs::read_to_string(root.join("descendant.pid"))
        .unwrap()
        .parse()
        .unwrap();
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, descendant) };
    if !process.is_null() {
        assert_eq!(unsafe { WaitForSingleObject(process, 0) }, WAIT_OBJECT_0);
        unsafe { CloseHandle(process) };
    }

    fs::write(&executable, b"replaced").unwrap();
    assert_eq!(
        prepare_review_provider_call(&provider, &request, &packet).unwrap_err(),
        ReviewProviderInvocationError::Unavailable
    );
}
