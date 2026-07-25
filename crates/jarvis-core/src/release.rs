//! Read-only release readiness and evidence inspection.
//!
//! This module inventories and structurally validates release artifacts and
//! owner-recorded evidence reports. It performs no signing, notarization,
//! stapling, installation, or live-device validation, and it holds no
//! repository, model, or execution authority.

use std::fs;
use std::path::{Path as FsPath, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const EXPECTED_MICROPHONE_USAGE_DESCRIPTION: &str =
    "Assemblywright uses microphone input only when you explicitly start local voice capture.";
const EXPECTED_SPEECH_RECOGNITION_USAGE_DESCRIPTION: &str =
    "Assemblywright uses speech recognition only to turn your spoken command into a local assistant request.";
const LIVE_DEVICE_QA_REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "evidence_type",
    "generated_at",
    "installed_app_path",
    "validation_flags.clean_profile",
    "validation_flags.finder_launch",
    "validation_flags.microphone",
    "validation_flags.speech_permission",
    "validation_flags.transcript_handoff",
    "validation_flags.audio_output",
    "validation_flags.notification",
    "validation_flags.restart",
    "validation_flags.manual_release_qa",
    "voice_loop.microphone_permission_prompt",
    "voice_loop.speech_permission_prompt",
    "voice_loop.spoken_transcript_handoff",
    "voice_loop.same_command_path",
    "voice_loop.speech_output_playback",
    "app_bundle.bundle_identifier",
    "app_bundle.short_version",
    "app_bundle.build_version",
    "app_bundle.microphone_usage_description",
    "app_bundle.speech_recognition_usage_description",
    "app_executable.executable_path",
    "app_executable.sha256",
    "app_executable.code_identifier",
    "app_executable.team_identifier",
    "app_executable.cdhash",
    "signed_provenance.report_path",
    "signed_provenance.sha256",
    "bundled_core.executable_path",
    "bundled_core.version",
    "bundled_core.sha256",
    "owner_recorded_live_voice_evidence.owner_name",
    "owner_recorded_live_voice_evidence.device_label",
    "owner_recorded_live_voice_evidence.profile_label",
    "owner_recorded_live_voice_evidence.voice_check_started_at",
    "owner_recorded_live_voice_evidence.voice_check_completed_at",
    "owner_recorded_live_voice_evidence.microphone_evidence_note",
    "owner_recorded_live_voice_evidence.speech_permission_evidence_note",
    "owner_recorded_live_voice_evidence.transcript_handoff_evidence_note",
    "owner_recorded_live_voice_evidence.audio_output_evidence_note",
    "owner_recorded_non_voice_evidence.clean_profile_evidence_note",
    "owner_recorded_non_voice_evidence.finder_launch_evidence_note",
    "owner_recorded_non_voice_evidence.notification_evidence_note",
    "owner_recorded_non_voice_evidence.notification_observed_at",
    "owner_recorded_non_voice_evidence.restart_evidence_note",
    "owner_recorded_non_voice_evidence.manual_release_qa_evidence_note",
    "notification_observation.kind",
    "notification_observation.title",
    "notification_observation.body",
    "notification_observation.thread_identifier",
    "notification_observation.observed_at",
    "voice_command_observation.test_phrase",
    "voice_command_observation.observed_transcript",
    "voice_command_observation.expected_command_text",
    "voice_command_observation.observed_command_text",
    "voice_command_observation.command_result_evidence_id",
    "voice_command_observation.audio_output_device_label",
    "proof_boundary",
];
const PLUGIN_TRUST_QA_REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "evidence_type",
    "generated_at",
    "review_source",
    "validation_flags.marketplace_review",
    "validation_flags.malware_scan",
    "validation_flags.os_sandbox",
    "validation_flags.egress_enforcement",
    "validation_flags.signed_publisher_policy",
    "validation_flags.manual_trust_review",
    "owner_recorded_plugin_trust_evidence.owner_name",
    "owner_recorded_plugin_trust_evidence.review_started_at",
    "owner_recorded_plugin_trust_evidence.review_completed_at",
    "owner_recorded_plugin_trust_evidence.marketplace_evidence_note",
    "owner_recorded_plugin_trust_evidence.malware_scan_evidence_note",
    "owner_recorded_plugin_trust_evidence.os_sandbox_evidence_note",
    "owner_recorded_plugin_trust_evidence.egress_evidence_note",
    "owner_recorded_plugin_trust_evidence.egress_policy_label",
    "owner_recorded_plugin_trust_evidence.egress_validation_completed_at",
    "owner_recorded_plugin_trust_evidence.egress_deny_fixture_evidence_note",
    "owner_recorded_plugin_trust_evidence.egress_allow_fixture_evidence_note",
    "owner_recorded_plugin_trust_evidence.signed_publisher_evidence_note",
    "owner_recorded_plugin_trust_evidence.manual_review_evidence_note",
    "evidence_artifacts.marketplace_review.uri",
    "evidence_artifacts.marketplace_review.sha256",
    "evidence_artifacts.malware_scan.uri",
    "evidence_artifacts.malware_scan.sha256",
    "evidence_artifacts.os_sandbox.uri",
    "evidence_artifacts.os_sandbox.sha256",
    "evidence_artifacts.egress_enforcement.uri",
    "evidence_artifacts.egress_enforcement.sha256",
    "evidence_artifacts.signed_publisher_policy.uri",
    "evidence_artifacts.signed_publisher_policy.sha256",
    "evidence_artifacts.manual_trust_review.uri",
    "evidence_artifacts.manual_trust_review.sha256",
    "proof_boundary",
];
const SIGNED_DISTRIBUTION_PROVENANCE_REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "evidence_type",
    "generated_at",
    "version",
    "bundle_identifier",
    "artifacts.app_path",
    "artifacts.zip_path",
    "artifacts.pkg_path",
    "artifacts.zip_sha256",
    "artifacts.pkg_sha256",
    "artifacts.app_executable_path",
    "artifacts.app_executable_sha256",
    "artifacts.bundled_core_path",
    "artifacts.bundled_core_sha256",
    "artifacts.bundled_core_version",
    "signing.developer_id_application_identity",
    "signing.developer_id_installer_identity",
    "signing.app_bundle_codesign",
    "signing.app_executable_codesign",
    "signing.app_executable_identifier",
    "signing.app_executable_team_identifier",
    "signing.app_executable_cdhash",
    "signing.bundled_core_codesign",
    "signing.installer_pkg_signature",
    "notarization.app_zip_submission_id",
    "notarization.installer_pkg_submission_id",
    "notarization.app_zip_status",
    "notarization.installer_pkg_status",
    "notarization.app_zip_notary_log",
    "notarization.installer_pkg_notary_log",
    "notarization.app_zip_notary_log_sha256",
    "notarization.installer_pkg_notary_log_sha256",
    "stapling.app_bundle_validation",
    "stapling.installer_pkg_validation",
    "gatekeeper.app_bundle_assessment",
    "gatekeeper.installer_pkg_assessment",
    "validation_flags.developer_id_application_signed",
    "validation_flags.developer_id_installer_signed",
    "validation_flags.app_zip_notarized",
    "validation_flags.installer_pkg_notarized",
    "validation_flags.app_stapled",
    "validation_flags.installer_pkg_stapled",
    "validation_flags.gatekeeper_assessed",
    "validation_flags.artifact_digests_recorded",
    "validation_flags.app_executable_identity_recorded",
    "proof_boundary",
];
const RELEASE_EVIDENCE_BUNDLE_REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "evidence_type",
    "generated_at",
    "version",
    "artifacts.app_path",
    "artifacts.zip_path",
    "artifacts.pkg_path",
    "artifacts.zip_sha256",
    "artifacts.pkg_sha256",
    "reports.signed_distribution_provenance_report",
    "reports.live_device_qa_report",
    "reports.plugin_trust_qa_report",
    "reports.signed_distribution_provenance_sha256",
    "reports.live_device_qa_sha256",
    "reports.plugin_trust_qa_sha256",
    "validation_flags.signed_distribution",
    "validation_flags.notarization",
    "validation_flags.clean_profile",
    "validation_flags.live_device_qa",
    "validation_flags.plugin_trust_qa",
    "validation_flags.reports_archived",
    "validation_flags.local_signature_validation",
    "owner_recorded_release_evidence.owner_name",
    "owner_recorded_release_evidence.completed_at",
    "owner_recorded_release_evidence.signed_distribution_note",
    "owner_recorded_release_evidence.notarization_note",
    "owner_recorded_release_evidence.clean_profile_note",
    "owner_recorded_release_evidence.live_device_qa_note",
    "owner_recorded_release_evidence.plugin_trust_qa_note",
    "owner_recorded_release_evidence.reports_archive_note",
    "owner_recorded_release_evidence.reports_archive_uri",
    "proof_boundary",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractFeature {
    pub key: String,
    pub status: String,
    pub proof: String,
    pub boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseReadinessResponse {
    pub generated_at: DateTime<Utc>,
    pub production_ready: bool,
    pub evidence_mode_enabled: bool,
    pub readiness_scope: String,
    pub verified_feature_count: usize,
    pub pending_feature_count: usize,
    pub implemented_features: Vec<ReleaseReadinessFeature>,
    pub pending_features: Vec<ReleaseReadinessFeature>,
    pub blocking_manual_gates: Vec<String>,
    pub recommended_verification_commands: Vec<String>,
    pub proof_boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseReadinessFeature {
    pub key: String,
    pub status: String,
    pub proof: String,
    pub boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseEvidenceStatusResponse {
    pub generated_at: DateTime<Utc>,
    pub complete: bool,
    pub satisfied_count: usize,
    pub missing_count: usize,
    pub invalid_count: usize,
    pub items: Vec<ReleaseEvidenceStatusItem>,
    pub proof_boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseEvidenceStatusItem {
    pub key: String,
    pub label: String,
    pub path: String,
    pub kind: ReleaseEvidenceKind,
    pub status: ReleaseEvidenceItemStatus,
    pub required_for_production: bool,
    pub manual_gate: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseRunbookResponse {
    pub generated_at: DateTime<Utc>,
    pub generated_from: String,
    pub runbook: String,
    pub production_ready: bool,
    pub live_voice_feature: Option<ReleaseReadinessFeature>,
    pub evidence_items: Vec<ReleaseEvidenceStatusItem>,
    pub commands: Vec<String>,
    pub manual_checks: Vec<String>,
    pub proof_boundary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseEvidenceKind {
    Directory,
    File,
    Executable,
    JsonReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseEvidenceItemStatus {
    Present,
    Missing,
    Invalid,
}

impl From<ContractFeature> for ReleaseReadinessFeature {
    fn from(feature: ContractFeature) -> Self {
        Self {
            key: feature.key,
            status: feature.status,
            proof: feature.proof,
            boundary: feature.boundary,
        }
    }
}

fn release_readiness_features(
    evidence_status: &ReleaseEvidenceStatusResponse,
    evidence_mode_enabled: bool,
) -> Vec<ContractFeature> {
    let live_device_qa_valid =
        release_live_device_evidence_valid(evidence_status, evidence_mode_enabled);
    contract_features()
        .into_iter()
        .map(|mut feature| {
            if feature.key == "live_voice_loop" && live_device_qa_valid {
                feature.status = "implemented".to_string();
                feature.proof = "A valid owner-recorded live-device QA report and valid signed-distribution provenance are present through explicitly enabled release evidence status, including exact app-executable digest/code-identity binding, microphone/Speech permission prompts, spoken transcript handoff into the command path, and speech-output playback evidence.".to_string();
                feature.boundary = "Owner-recorded live-device QA evidence with point-in-time binding to the referenced signed release candidate only; readiness still does not perform signing, notarization, stapling, installation, Finder/LaunchServices validation, live audio capture, continuous integrity monitoring, App Store review, marketplace review, malware analysis, or OS sandbox/egress enforcement.".to_string();
            }
            feature
        })
        .collect()
}

fn release_readiness_evidence_mode_enabled() -> bool {
    std::env::var("JARVIS_RELEASE_READINESS_EVIDENCE_MODE")
        .map(|value| value == "external")
        .unwrap_or(false)
}

fn release_evidence_item_present(status: &ReleaseEvidenceStatusResponse, key: &str) -> bool {
    status
        .items
        .iter()
        .any(|item| item.key == key && item.status == ReleaseEvidenceItemStatus::Present)
}

fn release_live_device_evidence_valid(
    evidence_status: &ReleaseEvidenceStatusResponse,
    evidence_mode_enabled: bool,
) -> bool {
    evidence_mode_enabled
        && release_evidence_item_present(evidence_status, "live_device_qa_report")
        && release_evidence_item_present(evidence_status, "signed_distribution_provenance_report")
}

fn release_production_ready(
    evidence_status: &ReleaseEvidenceStatusResponse,
    evidence_mode_enabled: bool,
    no_pending_features: bool,
) -> bool {
    evidence_mode_enabled
        && no_pending_features
        && release_required_evidence_complete(evidence_status)
}

fn release_required_evidence_complete(evidence_status: &ReleaseEvidenceStatusResponse) -> bool {
    const REQUIRED_RELEASE_EVIDENCE_KEYS: &[&str] = &[
        "signed_app_bundle",
        "app_executable",
        "bundled_core_executable",
        "signed_app_zip",
        "signed_installer_package",
        "signed_distribution_provenance_report",
        "live_device_qa_report",
        "plugin_trust_qa_report",
        "release_evidence_bundle",
    ];

    evidence_status.complete
        && REQUIRED_RELEASE_EVIDENCE_KEYS
            .iter()
            .all(|key| release_evidence_item_present(evidence_status, key))
}

fn release_live_device_runbook_from(
    readiness: &ReleaseReadinessResponse,
    evidence_status: &ReleaseEvidenceStatusResponse,
) -> ReleaseRunbookResponse {
    ReleaseRunbookResponse {
        generated_at: Utc::now(),
        generated_from: "release readiness plus evidence-status".to_string(),
        runbook: "live_device".to_string(),
        production_ready: readiness.production_ready,
        live_voice_feature: readiness
            .pending_features
            .iter()
            .chain(readiness.implemented_features.iter())
            .find(|feature| feature.key == "live_voice_loop")
            .cloned(),
        evidence_items: release_evidence_items_by_key(evidence_status, &["live_device_qa_report"]),
        commands: vec![
            "./scripts/release-live-device-qa.sh --check".to_string(),
            "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env"
                .to_string(),
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' in target/release-live-device-qa.env before collecting command evidence"
                .to_string(),
            "Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true for this operator evidence session, then confirm JARVIS_IPC_TOKEN_FILE points to the app-owned ipc-session-auth.json path before IPC commands"
                .to_string(),
            "cargo run -p jarvis-cli -- command \"status check\" --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\" --json"
                .to_string(),
            "Record the returned task ID as JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>' or a task-associated audit ID as 'audit:<uuid>' in target/release-live-device-qa.env"
                .to_string(),
            "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete"
                .to_string(),
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
            "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"
                .to_string(),
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
        ],
        manual_checks: vec![
            "Install the signed, notarized package into /Applications on a clean Mac profile."
                .to_string(),
            "Launch Assemblywright through Finder or LaunchServices.".to_string(),
            "Verify microphone and Speech permission prompts during live voice capture.".to_string(),
            "Speak the test phrase and confirm the observed transcript reaches the command path."
                .to_string(),
            "Verify live speech output, structured scheduler notification kind/title/body/thread evidence, restart behavior, and manual release QA."
                .to_string(),
            "Preserve target/release-live-device-qa-report.json for final release evidence bundling."
                .to_string(),
        ],
        proof_boundary:
            "Runbook and local evidence inspection only; this endpoint does not perform live-device validation."
                .to_string(),
    }
}

fn release_signed_distribution_runbook_from(
    readiness: &ReleaseReadinessResponse,
    evidence_status: &ReleaseEvidenceStatusResponse,
) -> ReleaseRunbookResponse {
    ReleaseRunbookResponse {
        generated_at: Utc::now(),
        generated_from: "release readiness plus evidence-status".to_string(),
        runbook: "signed_distribution".to_string(),
        production_ready: readiness.production_ready,
        live_voice_feature: None,
        evidence_items: release_evidence_items_by_key(
            evidence_status,
            &[
                "signed_app_bundle",
                "app_executable",
                "bundled_core_executable",
                "signed_app_zip",
                "signed_installer_package",
                "signed_distribution_provenance_report",
            ],
        ),
        commands: vec![
            "./scripts/package-distribution.sh --check".to_string(),
            "./scripts/package-distribution.sh --unsigned-launch-check".to_string(),
            "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh"
                .to_string(),
            "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_APPLE_ID='apple-id@example.com' JARVIS_NOTARYTOOL_TEAM_ID='TEAMID1234' JARVIS_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh"
                .to_string(),
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks"
                .to_string(),
            "Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true, then export JARVIS_IPC_TOKEN_FILE as the app-owned ipc-session-auth.json path before external IPC checks"
                .to_string(),
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
            "./scripts/release-evidence-doctor.sh --check".to_string(),
            "cargo run -p jarvis-cli -- release live-device-runbook".to_string(),
        ],
        manual_checks: vec![
            "Configure Developer ID Application and Installer identities plus either a notarytool keychain profile or Apple ID/team/app-specific password credentials on the release Mac."
                .to_string(),
            "Run the full package-distribution lane and preserve the signed zip, signed installer package, signed provenance report, and notarytool logs referenced by that report."
                .to_string(),
            "Confirm the signed installer package metadata still targets the Assemblywright package identifier, release version, and /Applications install location."
                .to_string(),
            "Confirm the signed app zip and installer package are notarized and stapled before clean-profile installation."
                .to_string(),
            "Rerun evidence-status and evidence-doctor so missing or invalid signed artifact paths are visible before final bundling."
                .to_string(),
            "Continue with live-device QA, plugin-trust QA, final evidence bundle generation, and external evidence-mode readiness."
                .to_string(),
        ],
        proof_boundary:
            "Runbook and local evidence inspection only; this endpoint does not perform signing, notarization, stapling, Gatekeeper assessment, installation, live-device QA, or plugin-trust QA."
                .to_string(),
    }
}

fn release_plugin_trust_runbook_from(
    readiness: &ReleaseReadinessResponse,
    evidence_status: &ReleaseEvidenceStatusResponse,
) -> ReleaseRunbookResponse {
    ReleaseRunbookResponse {
        generated_at: Utc::now(),
        generated_from: "release readiness plus evidence-status".to_string(),
        runbook: "plugin_trust".to_string(),
        production_ready: readiness.production_ready,
        live_voice_feature: None,
        evidence_items: release_evidence_items_by_key(evidence_status, &["plugin_trust_qa_report"]),
        commands: vec![
            "./scripts/release-plugin-trust-qa.sh --check".to_string(),
            "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env"
                .to_string(),
            "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete"
                .to_string(),
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks"
                .to_string(),
            "Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true, then export JARVIS_IPC_TOKEN_FILE as the app-owned ipc-session-auth.json path before external IPC checks"
                .to_string(),
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
            "./scripts/release-evidence-doctor.sh --check".to_string(),
            "./scripts/release-evidence-bundle.sh --check".to_string(),
            "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env"
                .to_string(),
            "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle"
                .to_string(),
            "./scripts/release-evidence-doctor.sh --assert-complete".to_string(),
        ],
        manual_checks: vec![
            "Run the marketplace review workflow for every public plugin listing.".to_string(),
            "Preserve malware scan evidence for distributed plugin archives and updates."
                .to_string(),
            "Validate signed publisher policy for trusted publisher keys and revocation."
                .to_string(),
            "Validate the macOS sandbox profile or equivalent OS-level confinement."
                .to_string(),
            "Validate host-level egress enforcement with deny and declared-host allow fixtures."
                .to_string(),
            "Record archived artifact URIs and SHA-256 digests for every plugin-trust evidence category before assertion."
                .to_string(),
            "Preserve target/release-plugin-trust-qa-report.json for final release evidence bundling."
                .to_string(),
            "Generate the final release evidence bundle only after signed distribution, live-device QA, and plugin-trust QA evidence all exist."
                .to_string(),
        ],
        proof_boundary:
            "Runbook and local evidence inspection only; this endpoint does not perform marketplace review, malware scanning, sandbox deployment, host-level egress enforcement, signing, notarization, live-device QA, or final evidence bundling."
                .to_string(),
    }
}

fn release_evidence_bundle_runbook_from(
    readiness: &ReleaseReadinessResponse,
    evidence_status: &ReleaseEvidenceStatusResponse,
) -> ReleaseRunbookResponse {
    ReleaseRunbookResponse {
        generated_at: Utc::now(),
        generated_from: "release readiness plus evidence-status".to_string(),
        runbook: "evidence_bundle".to_string(),
        production_ready: readiness.production_ready,
        live_voice_feature: None,
        evidence_items: release_evidence_items_by_key(
            evidence_status,
            &[
                "signed_distribution_provenance_report",
                "live_device_qa_report",
                "plugin_trust_qa_report",
                "release_evidence_bundle",
            ],
        ),
        commands: vec![
            "./scripts/release-evidence-bundle.sh --check".to_string(),
            "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env"
                .to_string(),
            "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle"
                .to_string(),
            "./scripts/release-evidence-doctor.sh --check".to_string(),
            "./scripts/release-evidence-doctor.sh --assert-complete".to_string(),
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks"
                .to_string(),
            "Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true, then export JARVIS_IPC_TOKEN_FILE as the app-owned ipc-session-auth.json path before external IPC checks"
                .to_string(),
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
            "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"
                .to_string(),
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
        ],
        manual_checks: vec![
            "Generate the final evidence bundle only after signed-distribution, live-device QA, and plugin-trust QA reports exist and have been archived."
                .to_string(),
            "Use a durable reports archive URI and preserve the signed zip, installer package, signed provenance report, live-device QA report, plugin-trust QA report, final bundle, and supporting logs."
                .to_string(),
            "Confirm release-evidence-doctor --assert-complete reports every required evidence item present before enabling external evidence-mode readiness."
                .to_string(),
            "Restart or start the release core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external before the final readiness check."
                .to_string(),
            "Confirm production_ready remains false if any required evidence item is missing, invalid, or stale."
                .to_string(),
        ],
        proof_boundary:
            "Runbook and local evidence inspection only; this endpoint does not generate the final bundle, sign, notarize, staple, install, Finder-launch, run live-device QA, perform marketplace review, scan malware, deploy a sandbox, or enforce host-level egress."
                .to_string(),
    }
}

fn release_evidence_items_by_key(
    evidence_status: &ReleaseEvidenceStatusResponse,
    keys: &[&str],
) -> Vec<ReleaseEvidenceStatusItem> {
    evidence_status
        .items
        .iter()
        .filter(|item| keys.contains(&item.key.as_str()))
        .cloned()
        .collect()
}

fn release_evidence_status_from_env() -> ReleaseEvidenceStatusResponse {
    release_evidence_status_from_env_inner()
}

fn release_evidence_status_from_env_inner() -> ReleaseEvidenceStatusResponse {
    let version = std::env::var("JARVIS_EVIDENCE_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let dist_dir = env_path("JARVIS_EVIDENCE_DIST_DIR", "target/distribution");
    let app_path = env_path_or(
        "JARVIS_EVIDENCE_APP_PATH",
        dist_dir.join("Assemblywright.app"),
    );
    let zip_path = env_path_or(
        "JARVIS_EVIDENCE_ZIP_PATH",
        dist_dir.join(format!("Assemblywright-{version}.zip")),
    );
    let pkg_path = env_path_or(
        "JARVIS_EVIDENCE_PKG_PATH",
        dist_dir.join(format!("Assemblywright-{version}.pkg")),
    );
    let live_qa_report = env_path_alias(
        "JARVIS_EVIDENCE_LIVE_QA_REPORT",
        "JARVIS_QA_REPORT_PATH",
        "target/release-live-device-qa-report.json",
    );
    let plugin_qa_report = env_path_alias(
        "JARVIS_EVIDENCE_PLUGIN_QA_REPORT",
        "JARVIS_PLUGIN_QA_REPORT_PATH",
        "target/release-plugin-trust-qa-report.json",
    );
    let bundle_path = env_path(
        "JARVIS_EVIDENCE_OUTPUT_PATH",
        "target/release-evidence-bundle.json",
    );
    let signed_provenance_report = env_path_or(
        "JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT",
        dist_dir.join(format!("Assemblywright-{version}-signed-provenance.json")),
    );

    let bundle_digest_paths = ReleaseEvidenceBundleDigestPaths {
        app_path: &app_path,
        zip_path: &zip_path,
        pkg_path: &pkg_path,
        signed_provenance_report: &signed_provenance_report,
        live_qa_report: &live_qa_report,
        plugin_qa_report: &plugin_qa_report,
    };

    let mut items = vec![
        release_app_bundle_item("signed_app_bundle", "App bundle path", app_path.clone()),
        release_path_item(
            "app_executable",
            "App executable",
            app_path.join("Contents/MacOS/JarvisMacApp"),
            ReleaseEvidenceKind::Executable,
        ),
        release_bundled_core_item(
            "bundled_core_executable",
            "Bundled core executable",
            app_path.join("Contents/Resources/bin/jarvis-cli"),
        ),
        release_path_item(
            "signed_app_zip",
            "App zip path",
            zip_path.clone(),
            ReleaseEvidenceKind::File,
        ),
        release_path_item(
            "signed_installer_package",
            "Installer package path",
            pkg_path.clone(),
            ReleaseEvidenceKind::File,
        ),
        release_signed_distribution_provenance_report_item(
            "signed_distribution_provenance_report",
            "Signed-distribution provenance report",
            signed_provenance_report.clone(),
            SIGNED_DISTRIBUTION_PROVENANCE_REQUIRED_FIELDS,
            app_path.clone(),
            zip_path.clone(),
            pkg_path.clone(),
        ),
        release_live_device_qa_report_item(
            "live_device_qa_report",
            "Live-device QA report",
            live_qa_report.clone(),
            LIVE_DEVICE_QA_REQUIRED_FIELDS,
            signed_provenance_report.clone(),
        ),
        release_json_report_item(
            "plugin_trust_qa_report",
            "Plugin-trust QA report",
            plugin_qa_report.clone(),
            PLUGIN_TRUST_QA_REQUIRED_FIELDS,
        ),
        release_evidence_bundle_report_item(
            "release_evidence_bundle",
            "Release evidence bundle",
            bundle_path,
            RELEASE_EVIDENCE_BUNDLE_REQUIRED_FIELDS,
            bundle_digest_paths,
        ),
    ];

    if dist_dir != FsPath::new("target/distribution") {
        items.push(release_path_item(
            "distribution_directory",
            "Distribution directory",
            dist_dir,
            ReleaseEvidenceKind::Directory,
        ));
    }

    let satisfied_count = items
        .iter()
        .filter(|item| item.status == ReleaseEvidenceItemStatus::Present)
        .count();
    let missing_count = items
        .iter()
        .filter(|item| item.status == ReleaseEvidenceItemStatus::Missing)
        .count();
    let invalid_count = items
        .iter()
        .filter(|item| item.status == ReleaseEvidenceItemStatus::Invalid)
        .count();

    ReleaseEvidenceStatusResponse {
        generated_at: Utc::now(),
        complete: missing_count == 0 && invalid_count == 0,
        satisfied_count,
        missing_count,
        invalid_count,
        items,
        proof_boundary:
            "File/report inventory only; complete means expected paths are present, app bundle metadata matches the expected bundle identifier/version/build and approved microphone/Speech privacy prompt copy, bundled core version-marker metadata matches the expected release version, and JSON reports pass required field checks plus signed-provenance artifact digest matching, live-device QA release-metadata/non-future timestamp semantics, exact app-executable SHA-256/code-identity and signed-provenance cross-report binding, required repository-backed task/audit command-result evidence resolution, plugin-trust non-future timestamp and owner-asserted review-source semantics, and final evidence-bundle path/digest/archive-URI/signature-validation/non-future timestamp semantics. This endpoint does not sign, notarize, staple, install, Finder-launch, execute release artifacts, run live-device QA, run marketplace review, scan malware, or enforce an OS sandbox/egress policy."
            .to_string(),
    }
}

fn env_path(key: &str, default: &str) -> PathBuf {
    env_path_or(key, PathBuf::from(default))
}

fn env_path_alias(primary: &str, alias: &str, default: &str) -> PathBuf {
    std::env::var(primary)
        .or_else(|_| std::env::var(alias))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

fn env_path_or(key: &str, default: PathBuf) -> PathBuf {
    std::env::var(key).map(PathBuf::from).unwrap_or(default)
}

fn release_path_item(
    key: &str,
    label: &str,
    path: PathBuf,
    kind: ReleaseEvidenceKind,
) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_path(&path, kind);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_app_bundle_item(key: &str, label: &str, path: PathBuf) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_app_bundle(&path);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::Directory,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_bundled_core_item(key: &str, label: &str, path: PathBuf) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_bundled_core(&path);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::Executable,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_json_report_item(
    key: &str,
    label: &str,
    path: PathBuf,
    required_fields: &[&str],
) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_json_report(key, &path, required_fields);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::JsonReport,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_live_device_qa_report_item(
    key: &str,
    label: &str,
    path: PathBuf,
    required_fields: &[&str],
    signed_provenance_path: PathBuf,
) -> ReleaseEvidenceStatusItem {
    let (mut status, mut detail) = inspect_release_json_report(key, &path, required_fields);
    if status == ReleaseEvidenceItemStatus::Present {
        let binding_result = (|| {
            let live_qa = read_release_evidence_child_report(&path, "live-device QA report")?;
            let signed_provenance = read_release_evidence_child_report(
                &signed_provenance_path,
                "signed-distribution provenance report",
            )?;
            validate_live_device_signed_provenance_binding(
                &live_qa,
                &signed_provenance,
                &signed_provenance_path,
            )
        })();
        if let Err(error) = binding_result {
            status = ReleaseEvidenceItemStatus::Invalid;
            detail = error;
        }
    }
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::JsonReport,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_signed_distribution_provenance_report_item(
    key: &str,
    label: &str,
    path: PathBuf,
    required_fields: &[&str],
    app_path: PathBuf,
    zip_path: PathBuf,
    pkg_path: PathBuf,
) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_json_report_with_artifacts(
        key,
        &path,
        required_fields,
        &app_path,
        &zip_path,
        &pkg_path,
    );
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::JsonReport,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_evidence_bundle_report_item(
    key: &str,
    label: &str,
    path: PathBuf,
    required_fields: &[&str],
    digest_paths: ReleaseEvidenceBundleDigestPaths<'_>,
) -> ReleaseEvidenceStatusItem {
    let (status, detail) =
        inspect_release_json_report_with_bundle_digests(key, &path, required_fields, digest_paths);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::JsonReport,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

#[derive(Clone, Copy)]
struct ReleaseEvidenceBundleDigestPaths<'a> {
    app_path: &'a FsPath,
    zip_path: &'a FsPath,
    pkg_path: &'a FsPath,
    signed_provenance_report: &'a FsPath,
    live_qa_report: &'a FsPath,
    plugin_qa_report: &'a FsPath,
}

fn inspect_release_path(
    path: &FsPath,
    kind: ReleaseEvidenceKind,
) -> (ReleaseEvidenceItemStatus, String) {
    const PRESENCE_ONLY_DETAIL: &str =
        "presence only; signing, notarization, and stapling are not validated by evidence-status";
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => {
            return (
                ReleaseEvidenceItemStatus::Missing,
                "expected evidence path is missing".to_string(),
            )
        }
    };

    match kind {
        ReleaseEvidenceKind::Directory if metadata.is_dir() => (
            ReleaseEvidenceItemStatus::Present,
            format!("directory exists; {PRESENCE_ONLY_DETAIL}"),
        ),
        ReleaseEvidenceKind::File if metadata.is_file() => (
            ReleaseEvidenceItemStatus::Present,
            format!("file exists; {PRESENCE_ONLY_DETAIL}"),
        ),
        ReleaseEvidenceKind::Executable if metadata.is_file() && is_executable(&metadata) => (
            ReleaseEvidenceItemStatus::Present,
            format!("executable file exists; {PRESENCE_ONLY_DETAIL}"),
        ),
        ReleaseEvidenceKind::Executable if metadata.is_file() => (
            ReleaseEvidenceItemStatus::Invalid,
            "file exists but is not executable".to_string(),
        ),
        _ => (
            ReleaseEvidenceItemStatus::Invalid,
            "path exists but has the wrong type".to_string(),
        ),
    }
}

fn inspect_release_app_bundle(path: &FsPath) -> (ReleaseEvidenceItemStatus, String) {
    let (status, detail) = inspect_release_path(path, ReleaseEvidenceKind::Directory);
    if status != ReleaseEvidenceItemStatus::Present {
        return (status, detail);
    }

    let info_plist = path.join("Contents/Info.plist");
    let contents = match fs::read_to_string(&info_plist) {
        Ok(contents) => contents,
        Err(_) => {
            return (
                ReleaseEvidenceItemStatus::Invalid,
                "app bundle Info.plist is missing or not readable as XML".to_string(),
            )
        }
    };

    let expected_bundle_id = expected_release_bundle_id();
    let expected_version = expected_release_evidence_version();
    for (key, expected) in [
        ("CFBundleIdentifier", expected_bundle_id.as_str()),
        ("CFBundleShortVersionString", expected_version.as_str()),
        ("CFBundleVersion", expected_version.as_str()),
        (
            "NSMicrophoneUsageDescription",
            EXPECTED_MICROPHONE_USAGE_DESCRIPTION,
        ),
        (
            "NSSpeechRecognitionUsageDescription",
            EXPECTED_SPEECH_RECOGNITION_USAGE_DESCRIPTION,
        ),
    ] {
        match plist_xml_string_value(&contents, key) {
            Some(actual) if actual == expected => {}
            Some(actual) => {
                return (
                    ReleaseEvidenceItemStatus::Invalid,
                    format!(
                        "app bundle Info.plist {key} mismatch: expected {expected}, got {actual}"
                    ),
                )
            }
            None => {
                return (
                    ReleaseEvidenceItemStatus::Invalid,
                    format!("app bundle Info.plist missing {key}"),
                )
            }
        }
    }

    (
        ReleaseEvidenceItemStatus::Present,
        "directory exists; Info.plist bundle identifier, short version, build version, and privacy prompt copy match expected release metadata; signing, notarization, and stapling are not validated by evidence-status".to_string(),
    )
}

fn inspect_release_bundled_core(path: &FsPath) -> (ReleaseEvidenceItemStatus, String) {
    let (status, detail) = inspect_release_path(path, ReleaseEvidenceKind::Executable);
    if status != ReleaseEvidenceItemStatus::Present {
        return (status, detail);
    }

    let remediation =
        "rerun ./scripts/package-distribution.sh --unsigned-launch-check for local evidence, \
         or the signed package-distribution.sh lane before final release evidence";
    let version_marker = path.with_file_name(format!(
        "{}.version",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("jarvis-cli")
    ));
    let version = match fs::read_to_string(&version_marker) {
        Ok(version) => version,
        Err(_) => {
            return (
                ReleaseEvidenceItemStatus::Invalid,
                format!("bundled core version marker is missing or not readable; {remediation}"),
            )
        }
    };
    let expected_version = format!("assemblywright {}", expected_release_evidence_version());
    if version.trim() != expected_version {
        return (
            ReleaseEvidenceItemStatus::Invalid,
            format!(
                "bundled core version marker mismatch: expected {expected_version}, observed {}; {remediation}",
                version.trim()
            ),
        );
    }

    (
        ReleaseEvidenceItemStatus::Present,
        "executable file exists; bundled core version marker matches expected release version; signing, notarization, and stapling are not validated by evidence-status".to_string(),
    )
}

fn plist_xml_string_value(contents: &str, key: &str) -> Option<String> {
    let key_marker = format!("<key>{key}</key>");
    let after_key = contents.split_once(&key_marker)?.1;
    let after_string = after_key.split_once("<string>")?.1;
    let value = after_string.split_once("</string>")?.0;
    Some(value.trim().to_string())
}

fn inspect_release_json_report(
    key: &str,
    path: &FsPath,
    required_fields: &[&str],
) -> (ReleaseEvidenceItemStatus, String) {
    inspect_release_json_report_inner(key, path, required_fields, None, None)
}

fn inspect_release_json_report_with_artifacts(
    key: &str,
    path: &FsPath,
    required_fields: &[&str],
    app_path: &FsPath,
    zip_path: &FsPath,
    pkg_path: &FsPath,
) -> (ReleaseEvidenceItemStatus, String) {
    inspect_release_json_report_inner(
        key,
        path,
        required_fields,
        Some((app_path, zip_path, pkg_path)),
        None,
    )
}

fn inspect_release_json_report_with_bundle_digests(
    key: &str,
    path: &FsPath,
    required_fields: &[&str],
    bundle_digest_paths: ReleaseEvidenceBundleDigestPaths<'_>,
) -> (ReleaseEvidenceItemStatus, String) {
    inspect_release_json_report_inner(key, path, required_fields, None, Some(bundle_digest_paths))
}

fn inspect_release_json_report_inner(
    key: &str,
    path: &FsPath,
    required_fields: &[&str],
    signed_artifact_paths: Option<(&FsPath, &FsPath, &FsPath)>,
    bundle_digest_paths: Option<ReleaseEvidenceBundleDigestPaths<'_>>,
) -> (ReleaseEvidenceItemStatus, String) {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) => {
            return (
                ReleaseEvidenceItemStatus::Missing,
                "expected JSON report is missing".to_string(),
            )
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(value) => value,
        Err(error) => {
            return (
                ReleaseEvidenceItemStatus::Invalid,
                format!("JSON report is invalid: {error}"),
            )
        }
    };
    let missing = required_fields
        .iter()
        .copied()
        .filter(|field| !json_field_is_present(&value, field))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        if key == "live_device_qa_report" {
            if let Err(error) = validate_live_device_qa_report(&value) {
                return (ReleaseEvidenceItemStatus::Invalid, error);
            }
        } else if key == "plugin_trust_qa_report" {
            if let Err(error) = validate_plugin_trust_qa_report(&value) {
                return (ReleaseEvidenceItemStatus::Invalid, error);
            }
        } else if key == "signed_distribution_provenance_report" {
            if let Err(error) = validate_signed_distribution_provenance(&value) {
                return (ReleaseEvidenceItemStatus::Invalid, error);
            }
            if let Some((app_path, zip_path, pkg_path)) = signed_artifact_paths {
                if let Err(error) = validate_signed_distribution_artifact_digests(
                    &value, app_path, zip_path, pkg_path,
                ) {
                    return (ReleaseEvidenceItemStatus::Invalid, error);
                }
            }
        } else if key == "release_evidence_bundle" {
            if let Err(error) = validate_release_evidence_bundle(&value) {
                return (ReleaseEvidenceItemStatus::Invalid, error);
            }
            if let Some(paths) = bundle_digest_paths {
                if let Err(error) = validate_release_evidence_bundle_file_bindings(&value, paths) {
                    return (ReleaseEvidenceItemStatus::Invalid, error);
                }
            }
        }
        (
            ReleaseEvidenceItemStatus::Present,
            release_json_present_detail(key),
        )
    } else {
        (
            ReleaseEvidenceItemStatus::Invalid,
            format!(
                "JSON report is missing required fields: {}",
                missing.join(", ")
            ),
        )
    }
}

fn validate_live_device_qa_report(value: &serde_json::Value) -> Result<(), String> {
    let generated_at = require_utc_report_timestamp_not_future(value, "generated_at")?;
    if value
        .get("schema_version")
        .and_then(|schema| schema.as_i64())
        != Some(1)
    {
        return Err("JSON report schema_version must be 1".to_string());
    }
    if value.get("evidence_type").and_then(|kind| kind.as_str())
        != Some("owner_recorded_live_device_qa")
    {
        return Err("JSON report evidence_type must be owner_recorded_live_device_qa".to_string());
    }
    if value
        .get("self_test_fixture")
        .and_then(|fixture| fixture.as_bool())
        .unwrap_or(true)
    {
        return Err("JSON report must not be marked as a self-test fixture".to_string());
    }

    let expected_bundle_id = env_value_alias(
        "JARVIS_QA_EXPECTED_BUNDLE_ID",
        "JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID",
        "com.nobiletechnology.jarvis",
    );
    let expected_version = expected_live_qa_version();
    require_json_string_value(value, "app_bundle.bundle_identifier", &expected_bundle_id)?;
    require_json_string_value(value, "app_bundle.short_version", &expected_version)?;
    require_json_string_value(value, "app_bundle.build_version", &expected_version)?;
    let expected_installed_app_path = std::env::var("JARVIS_QA_INSTALLED_APP_PATH")
        .unwrap_or_else(|_| "/Applications/Assemblywright.app".to_string());
    require_json_string_value(value, "installed_app_path", &expected_installed_app_path)?;
    require_json_string_value(
        value,
        "app_executable.executable_path",
        &format!("{expected_installed_app_path}/Contents/MacOS/JarvisMacApp"),
    )?;
    require_json_sha256_value(value, "app_executable.sha256")?;
    require_json_string_value(value, "app_executable.code_identifier", &expected_bundle_id)?;
    require_json_team_identifier_value(value, "app_executable.team_identifier")?;
    require_json_cdhash_value(value, "app_executable.cdhash")?;
    require_json_nonempty_string_value(value, "signed_provenance.report_path")?;
    require_json_sha256_value(value, "signed_provenance.sha256")?;
    require_json_string_value(
        value,
        "bundled_core.executable_path",
        &format!("{expected_installed_app_path}/Contents/Resources/bin/jarvis-cli"),
    )?;
    require_json_string_value(
        value,
        "bundled_core.version",
        &format!("assemblywright {expected_version}"),
    )?;
    require_json_sha256_value(value, "bundled_core.sha256")?;
    for field in [
        "validation_flags.clean_profile",
        "validation_flags.finder_launch",
        "validation_flags.microphone",
        "validation_flags.speech_permission",
        "validation_flags.transcript_handoff",
        "validation_flags.audio_output",
        "validation_flags.notification",
        "validation_flags.restart",
        "validation_flags.manual_release_qa",
        "voice_loop.microphone_permission_prompt",
        "voice_loop.speech_permission_prompt",
        "voice_loop.spoken_transcript_handoff",
        "voice_loop.same_command_path",
        "voice_loop.speech_output_playback",
    ] {
        require_json_bool_value(value, field, true)?;
    }
    for field in [
        "owner_recorded_live_voice_evidence.owner_name",
        "owner_recorded_live_voice_evidence.device_label",
        "owner_recorded_live_voice_evidence.profile_label",
        "voice_command_observation.test_phrase",
        "voice_command_observation.observed_transcript",
        "voice_command_observation.expected_command_text",
        "voice_command_observation.observed_command_text",
        "voice_command_observation.audio_output_device_label",
        "notification_observation.title",
        "notification_observation.body",
        "proof_boundary",
    ] {
        require_json_nonempty_string_value(value, field)?;
    }
    require_json_string_value(
        value,
        "app_bundle.microphone_usage_description",
        EXPECTED_MICROPHONE_USAGE_DESCRIPTION,
    )?;
    require_json_string_value(
        value,
        "app_bundle.speech_recognition_usage_description",
        EXPECTED_SPEECH_RECOGNITION_USAGE_DESCRIPTION,
    )?;
    require_json_string_value(
        value,
        "notification_observation.thread_identifier",
        "jarvis.scheduler",
    )?;
    let notification_kind =
        json_string_at(value, "notification_observation.kind").ok_or_else(|| {
            "JSON report is missing required field: notification_observation.kind".to_string()
        })?;
    if !matches!(
        notification_kind.as_str(),
        "due_now" | "failed" | "blocked_by_emergency_pause"
    ) {
        return Err("JSON report notification_observation.kind must be due_now, failed, or blocked_by_emergency_pause".to_string());
    }
    for field in [
        "owner_recorded_live_voice_evidence.microphone_evidence_note",
        "owner_recorded_live_voice_evidence.speech_permission_evidence_note",
        "owner_recorded_live_voice_evidence.transcript_handoff_evidence_note",
        "owner_recorded_live_voice_evidence.audio_output_evidence_note",
        "owner_recorded_non_voice_evidence.clean_profile_evidence_note",
        "owner_recorded_non_voice_evidence.finder_launch_evidence_note",
        "owner_recorded_non_voice_evidence.notification_evidence_note",
        "owner_recorded_non_voice_evidence.restart_evidence_note",
        "owner_recorded_non_voice_evidence.manual_release_qa_evidence_note",
    ] {
        require_json_meaningful_owner_evidence(value, field)?;
    }

    let started_at = require_utc_report_timestamp(
        value,
        "owner_recorded_live_voice_evidence.voice_check_started_at",
    )?;
    let completed_at = require_utc_report_timestamp(
        value,
        "owner_recorded_live_voice_evidence.voice_check_completed_at",
    )?;
    if completed_at < started_at {
        return Err("JSON report voice_check_completed_at must be greater than or equal to voice_check_started_at".to_string());
    }
    let notification_observed_at = require_utc_report_timestamp(
        value,
        "owner_recorded_non_voice_evidence.notification_observed_at",
    )?;
    let notification_payload_observed_at =
        require_utc_report_timestamp(value, "notification_observation.observed_at")?;
    if notification_payload_observed_at != notification_observed_at {
        return Err("JSON report notification_observation.observed_at must match owner_recorded_non_voice_evidence.notification_observed_at".to_string());
    }
    if notification_observed_at < started_at {
        return Err("JSON report notification_observed_at must be greater than or equal to voice_check_started_at".to_string());
    }
    if generated_at < completed_at {
        return Err(
            "JSON report generated_at must be greater than or equal to voice_check_completed_at"
                .to_string(),
        );
    }
    if generated_at < notification_observed_at {
        return Err(
            "JSON report generated_at must be greater than or equal to notification_observed_at"
                .to_string(),
        );
    }

    let expected_command = json_string_at(value, "voice_command_observation.expected_command_text")
        .ok_or_else(|| {
            "JSON report is missing required field: voice_command_observation.expected_command_text"
                .to_string()
        })?;
    let observed_command = json_string_at(value, "voice_command_observation.observed_command_text")
        .ok_or_else(|| {
            "JSON report is missing required field: voice_command_observation.observed_command_text"
                .to_string()
        })?;
    let test_phrase =
        json_string_at(value, "voice_command_observation.test_phrase").ok_or_else(|| {
            "JSON report is missing required field: voice_command_observation.test_phrase"
                .to_string()
        })?;
    let observed_transcript = json_string_at(
        value,
        "voice_command_observation.observed_transcript",
    )
    .ok_or_else(|| {
        "JSON report is missing required field: voice_command_observation.observed_transcript"
            .to_string()
    })?;
    if test_phrase.trim() != observed_transcript.trim() {
        return Err(
            "JSON report observed_transcript must match test_phrase after trimming whitespace"
                .to_string(),
        );
    }
    if expected_command.trim() != observed_command.trim() {
        return Err(
            "JSON report observed_command_text must match expected_command_text".to_string(),
        );
    }
    let command_result_evidence_id = json_string_at(
        value,
        "voice_command_observation.command_result_evidence_id",
    )
    .ok_or_else(|| {
        "JSON report is missing required field: voice_command_observation.command_result_evidence_id"
            .to_string()
    })?;
    validate_command_result_evidence_id(&command_result_evidence_id)?;

    Ok(())
}

fn validate_command_result_evidence_id(value: &str) -> Result<(), String> {
    let (kind, id) = value.trim().split_once(':').ok_or_else(|| {
        "JSON report command_result_evidence_id must be task:<uuid> or audit:<uuid>".to_string()
    })?;
    if kind != "task" && kind != "audit" {
        return Err(
            "JSON report command_result_evidence_id must be task:<uuid> or audit:<uuid>"
                .to_string(),
        );
    }
    Uuid::parse_str(id).map_err(|_| {
        "JSON report command_result_evidence_id must be task:<uuid> or audit:<uuid>".to_string()
    })?;
    Ok(())
}

fn validate_plugin_trust_qa_report(value: &serde_json::Value) -> Result<(), String> {
    let generated_at = require_utc_report_timestamp_not_future(value, "generated_at")?;
    if value
        .get("schema_version")
        .and_then(|schema| schema.as_i64())
        != Some(1)
    {
        return Err("JSON report schema_version must be 1".to_string());
    }
    if value.get("evidence_type").and_then(|kind| kind.as_str())
        != Some("owner_recorded_plugin_trust_qa")
    {
        return Err("JSON report evidence_type must be owner_recorded_plugin_trust_qa".to_string());
    }
    require_json_string_value(value, "version", &expected_release_evidence_version())?;
    require_json_bool_value(value, "self_test_fixture", false)?;
    require_json_string_value(value, "review_source", "owner-asserted-manual-review")?;
    for field in [
        "validation_flags.marketplace_review",
        "validation_flags.malware_scan",
        "validation_flags.os_sandbox",
        "validation_flags.egress_enforcement",
        "validation_flags.signed_publisher_policy",
        "validation_flags.manual_trust_review",
    ] {
        require_json_bool_value(value, field, true)?;
    }
    let started_at = require_utc_report_timestamp(
        value,
        "owner_recorded_plugin_trust_evidence.review_started_at",
    )?;
    let completed_at = require_utc_report_timestamp(
        value,
        "owner_recorded_plugin_trust_evidence.review_completed_at",
    )?;
    let egress_completed_at = require_utc_report_timestamp(
        value,
        "owner_recorded_plugin_trust_evidence.egress_validation_completed_at",
    )?;
    for field in [
        "owner_recorded_plugin_trust_evidence.marketplace_evidence_note",
        "owner_recorded_plugin_trust_evidence.malware_scan_evidence_note",
        "owner_recorded_plugin_trust_evidence.os_sandbox_evidence_note",
        "owner_recorded_plugin_trust_evidence.egress_evidence_note",
        "owner_recorded_plugin_trust_evidence.egress_policy_label",
        "owner_recorded_plugin_trust_evidence.egress_deny_fixture_evidence_note",
        "owner_recorded_plugin_trust_evidence.egress_allow_fixture_evidence_note",
        "owner_recorded_plugin_trust_evidence.signed_publisher_evidence_note",
        "owner_recorded_plugin_trust_evidence.manual_review_evidence_note",
    ] {
        require_json_meaningful_owner_evidence(value, field)?;
    }
    for artifact in [
        "marketplace_review",
        "malware_scan",
        "os_sandbox",
        "egress_enforcement",
        "signed_publisher_policy",
        "manual_trust_review",
    ] {
        require_json_durable_evidence_archive_uri_value(
            value,
            &format!("evidence_artifacts.{artifact}.uri"),
        )?;
        require_json_sha256_value(value, &format!("evidence_artifacts.{artifact}.sha256"))?;
    }
    if completed_at < started_at {
        return Err(
            "JSON report review_completed_at must be greater than or equal to review_started_at"
                .to_string(),
        );
    }
    if egress_completed_at < started_at {
        return Err(
            "JSON report egress_validation_completed_at must be greater than or equal to review_started_at"
                .to_string(),
        );
    }
    if completed_at < egress_completed_at {
        return Err(
            "JSON report review_completed_at must be greater than or equal to egress_validation_completed_at"
                .to_string(),
        );
    }
    if generated_at < completed_at {
        return Err(
            "JSON report generated_at must be greater than or equal to review_completed_at"
                .to_string(),
        );
    }
    Ok(())
}

fn require_json_meaningful_owner_evidence(
    value: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    require_json_nonempty_string_value(value, path)?;
    let evidence = json_string_at(value, path)
        .ok_or_else(|| format!("JSON report is missing required field: {path}"))?;
    let normalized = evidence
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let placeholder = matches!(normalized.as_str(), "n/a" | "na")
        || [
            "self-test",
            "placeholder",
            "example",
            "fixture",
            "todo",
            "tbd",
            "replace-me",
            "changeme",
            "pending",
        ]
        .iter()
        .any(|marker| normalized.contains(marker));
    if placeholder {
        return Err(format!(
            "JSON report {path} must contain owner-recorded external evidence, not placeholder or fixture text"
        ));
    }
    Ok(())
}

fn validate_signed_distribution_provenance(value: &serde_json::Value) -> Result<(), String> {
    require_utc_report_timestamp_not_future(value, "generated_at")?;
    if value
        .get("schema_version")
        .and_then(|schema| schema.as_i64())
        != Some(1)
    {
        return Err("JSON report schema_version must be 1".to_string());
    }
    if value.get("evidence_type").and_then(|kind| kind.as_str())
        != Some("signed_distribution_provenance")
    {
        return Err("JSON report evidence_type must be signed_distribution_provenance".to_string());
    }
    require_json_string_value(value, "version", &expected_release_evidence_version())?;
    let expected_app_path = env_path_or(
        "JARVIS_EVIDENCE_APP_PATH",
        env_path("JARVIS_EVIDENCE_DIST_DIR", "target/distribution").join("Assemblywright.app"),
    );
    require_json_string_value(
        value,
        "artifacts.app_executable_path",
        &expected_app_path
            .join("Contents/MacOS/JarvisMacApp")
            .display()
            .to_string(),
    )?;
    require_json_sha256_value(value, "artifacts.app_executable_sha256")?;
    require_json_string_value(
        value,
        "artifacts.bundled_core_path",
        &expected_app_path
            .join("Contents/Resources/bin/jarvis-cli")
            .display()
            .to_string(),
    )?;
    require_json_string_value(
        value,
        "artifacts.bundled_core_version",
        &format!("assemblywright {}", expected_release_evidence_version()),
    )?;
    require_json_string_value(
        value,
        "bundle_identifier",
        &env_value_alias(
            "JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID",
            "JARVIS_QA_EXPECTED_BUNDLE_ID",
            "com.nobiletechnology.jarvis",
        ),
    )?;
    let expected_bundle_identifier = env_value_alias(
        "JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID",
        "JARVIS_QA_EXPECTED_BUNDLE_ID",
        "com.nobiletechnology.jarvis",
    );
    require_json_string_value(
        value,
        "signing.app_executable_identifier",
        &expected_bundle_identifier,
    )?;
    require_json_team_identifier_value(value, "signing.app_executable_team_identifier")?;
    require_json_cdhash_value(value, "signing.app_executable_cdhash")?;
    for field in [
        "artifacts.zip_sha256",
        "artifacts.pkg_sha256",
        "artifacts.app_executable_sha256",
        "artifacts.bundled_core_sha256",
        "notarization.app_zip_notary_log_sha256",
        "notarization.installer_pkg_notary_log_sha256",
    ] {
        require_json_sha256_value(value, field)?;
    }
    require_json_string_prefix_value(
        value,
        "signing.developer_id_application_identity",
        "Developer ID Application: ",
    )?;
    require_json_string_prefix_value(
        value,
        "signing.developer_id_installer_identity",
        "Developer ID Installer: ",
    )?;
    for field in [
        "signing.app_bundle_codesign",
        "signing.app_executable_codesign",
        "signing.bundled_core_codesign",
    ] {
        require_json_string_contains_value(value, field, "Authority=Developer ID Application: ")?;
    }
    require_json_string_contains_value(
        value,
        "signing.installer_pkg_signature",
        "Developer ID Installer: ",
    )?;
    for field in [
        "notarization.app_zip_submission_id",
        "notarization.installer_pkg_submission_id",
    ] {
        require_json_uuid_value(value, field)?;
    }
    require_json_string_value(value, "notarization.app_zip_status", "Accepted")?;
    require_json_string_value(value, "notarization.installer_pkg_status", "Accepted")?;
    for field in [
        "notarization.app_zip_notary_log",
        "notarization.installer_pkg_notary_log",
    ] {
        require_json_nonempty_string_value(value, field)?;
    }
    for field in [
        "stapling.app_bundle_validation",
        "stapling.installer_pkg_validation",
    ] {
        require_json_string_contains_value(value, field, "The validate action worked!")?;
    }
    for field in [
        "gatekeeper.app_bundle_assessment",
        "gatekeeper.installer_pkg_assessment",
    ] {
        require_gatekeeper_accepted_value(value, field)?;
    }
    for field in [
        "validation_flags.developer_id_application_signed",
        "validation_flags.developer_id_installer_signed",
        "validation_flags.app_zip_notarized",
        "validation_flags.installer_pkg_notarized",
        "validation_flags.app_stapled",
        "validation_flags.installer_pkg_stapled",
        "validation_flags.gatekeeper_assessed",
        "validation_flags.artifact_digests_recorded",
        "validation_flags.app_executable_identity_recorded",
    ] {
        require_json_bool_value(value, field, true)?;
    }

    Ok(())
}

fn validate_signed_distribution_artifact_digests(
    value: &serde_json::Value,
    app_path: &FsPath,
    zip_path: &FsPath,
    pkg_path: &FsPath,
) -> Result<(), String> {
    require_json_sha256_matches_file(value, "artifacts.zip_sha256", "app zip artifact", zip_path)?;
    require_json_sha256_matches_file(
        value,
        "artifacts.pkg_sha256",
        "installer package artifact",
        pkg_path,
    )?;
    require_json_sha256_matches_file(
        value,
        "artifacts.app_executable_sha256",
        "app executable",
        &app_path.join("Contents/MacOS/JarvisMacApp"),
    )?;
    require_json_sha256_matches_file(
        value,
        "artifacts.bundled_core_sha256",
        "bundled core executable",
        &app_path.join("Contents/Resources/bin/jarvis-cli"),
    )?;
    require_json_sha256_matches_json_path(
        value,
        "notarization.app_zip_notary_log_sha256",
        "app zip notary log",
        "notarization.app_zip_notary_log",
    )?;
    require_json_sha256_matches_json_path(
        value,
        "notarization.installer_pkg_notary_log_sha256",
        "installer package notary log",
        "notarization.installer_pkg_notary_log",
    )?;
    Ok(())
}

fn require_json_sha256_matches_json_path(
    value: &serde_json::Value,
    digest_path: &str,
    artifact_label: &str,
    artifact_path_field: &str,
) -> Result<(), String> {
    let artifact_path = json_string_at(value, artifact_path_field)
        .ok_or_else(|| format!("JSON report is missing required field: {artifact_path_field}"))?;
    if artifact_path.trim().is_empty() {
        return Err(format!(
            "JSON report {artifact_path_field} must be non-empty"
        ));
    }
    require_json_sha256_matches_file(
        value,
        digest_path,
        artifact_label,
        FsPath::new(&artifact_path),
    )
}

fn require_json_sha256_matches_file(
    value: &serde_json::Value,
    dotted_path: &str,
    artifact_label: &str,
    artifact_path: &FsPath,
) -> Result<(), String> {
    let expected = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    let actual = file_sha256(artifact_path).map_err(|error| {
        format!(
            "JSON report {dotted_path} cannot be checked because current {artifact_label} {} is unreadable: {error}",
            artifact_path.display()
        )
    })?;
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} does not match current {artifact_label} {}",
            artifact_path.display()
        ))
    }
}

fn file_sha256(path: &FsPath) -> std::io::Result<String> {
    let contents = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(&contents)))
}

fn validate_release_evidence_bundle(value: &serde_json::Value) -> Result<(), String> {
    let generated_at = require_utc_report_timestamp_not_future(value, "generated_at")?;
    if value
        .get("schema_version")
        .and_then(|schema| schema.as_i64())
        != Some(1)
    {
        return Err("JSON report schema_version must be 1".to_string());
    }
    if value.get("evidence_type").and_then(|kind| kind.as_str()) != Some("release_evidence_bundle")
    {
        return Err("JSON report evidence_type must be release_evidence_bundle".to_string());
    }
    require_json_string_value(value, "version", &expected_release_evidence_version())?;
    for field in [
        "validation_flags.signed_distribution",
        "validation_flags.notarization",
        "validation_flags.clean_profile",
        "validation_flags.live_device_qa",
        "validation_flags.plugin_trust_qa",
        "validation_flags.reports_archived",
        "validation_flags.local_signature_validation",
    ] {
        require_json_bool_value(value, field, true)?;
    }
    for field in [
        "artifacts.zip_sha256",
        "artifacts.pkg_sha256",
        "reports.signed_distribution_provenance_sha256",
        "reports.live_device_qa_sha256",
        "reports.plugin_trust_qa_sha256",
    ] {
        require_json_sha256_value(value, field)?;
    }
    for field in [
        "owner_recorded_release_evidence.owner_name",
        "owner_recorded_release_evidence.reports_archive_uri",
        "proof_boundary",
    ] {
        require_json_nonempty_string_value(value, field)?;
    }
    for field in [
        "owner_recorded_release_evidence.signed_distribution_note",
        "owner_recorded_release_evidence.notarization_note",
        "owner_recorded_release_evidence.clean_profile_note",
        "owner_recorded_release_evidence.live_device_qa_note",
        "owner_recorded_release_evidence.plugin_trust_qa_note",
        "owner_recorded_release_evidence.reports_archive_note",
    ] {
        require_json_meaningful_owner_evidence(value, field)?;
    }
    require_json_release_reports_archive_uri_value(
        value,
        "owner_recorded_release_evidence.reports_archive_uri",
    )?;
    let completed_at =
        require_utc_report_timestamp(value, "owner_recorded_release_evidence.completed_at")?;
    if generated_at < completed_at {
        return Err(
            "JSON report generated_at must be greater than or equal to owner_recorded_release_evidence.completed_at"
                .to_string(),
        );
    }

    Ok(())
}

fn validate_live_device_signed_provenance_binding(
    live_qa: &serde_json::Value,
    signed_provenance: &serde_json::Value,
    signed_provenance_path: &FsPath,
) -> Result<(), String> {
    let expected_signed_provenance_path = signed_provenance_path.display().to_string();
    let live_signed_provenance_path = json_string_at(live_qa, "signed_provenance.report_path")
        .ok_or_else(|| {
            "live-device QA report is missing signed_provenance.report_path".to_string()
        })?;
    if live_signed_provenance_path != expected_signed_provenance_path {
        return Err(
            "live-device QA report signed_provenance.report_path does not match the configured signed-distribution provenance report"
                .to_string(),
        );
    }
    require_json_sha256_matches_file(
        live_qa,
        "signed_provenance.sha256",
        "signed-distribution provenance report",
        signed_provenance_path,
    )?;

    for (live_field, signed_field, label) in [
        (
            "bundled_core.sha256",
            "artifacts.bundled_core_sha256",
            "bundled_core.sha256",
        ),
        (
            "app_executable.sha256",
            "artifacts.app_executable_sha256",
            "app executable SHA-256",
        ),
        (
            "app_executable.code_identifier",
            "signing.app_executable_identifier",
            "app executable code identifier",
        ),
        (
            "app_executable.team_identifier",
            "signing.app_executable_team_identifier",
            "app executable team identifier",
        ),
        (
            "app_executable.cdhash",
            "signing.app_executable_cdhash",
            "app executable CDHash",
        ),
    ] {
        let live_value = json_string_at(live_qa, live_field)
            .ok_or_else(|| format!("live-device QA report is missing {live_field}"))?;
        let signed_value = json_string_at(signed_provenance, signed_field).ok_or_else(|| {
            format!("signed-distribution provenance report is missing {signed_field}")
        })?;
        if live_value != signed_value {
            return Err(format!(
                "live-device QA report {label} does not match signed-distribution provenance"
            ));
        }
    }
    Ok(())
}

fn validate_release_evidence_bundle_file_bindings(
    value: &serde_json::Value,
    paths: ReleaseEvidenceBundleDigestPaths<'_>,
) -> Result<(), String> {
    require_json_string_value(
        value,
        "artifacts.app_path",
        &paths.app_path.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "artifacts.zip_path",
        &paths.zip_path.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "artifacts.pkg_path",
        &paths.pkg_path.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "reports.signed_distribution_provenance_report",
        &paths.signed_provenance_report.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "reports.live_device_qa_report",
        &paths.live_qa_report.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "reports.plugin_trust_qa_report",
        &paths.plugin_qa_report.display().to_string(),
    )?;
    require_json_sha256_matches_file(
        value,
        "artifacts.zip_sha256",
        "app zip artifact",
        paths.zip_path,
    )?;
    require_json_sha256_matches_file(
        value,
        "artifacts.pkg_sha256",
        "installer package artifact",
        paths.pkg_path,
    )?;
    require_json_sha256_matches_file(
        value,
        "reports.signed_distribution_provenance_sha256",
        "signed-distribution provenance report",
        paths.signed_provenance_report,
    )?;
    require_json_sha256_matches_file(
        value,
        "reports.live_device_qa_sha256",
        "live-device QA report",
        paths.live_qa_report,
    )?;
    require_json_sha256_matches_file(
        value,
        "reports.plugin_trust_qa_sha256",
        "plugin-trust QA report",
        paths.plugin_qa_report,
    )?;
    let signed_provenance = read_release_evidence_child_report(
        paths.signed_provenance_report,
        "signed-distribution provenance report",
    )?;
    validate_signed_distribution_provenance(&signed_provenance).map_err(|error| {
        format!("signed-distribution provenance report referenced by release evidence bundle is invalid: {error}")
    })?;
    validate_signed_distribution_artifact_digests(
        &signed_provenance,
        paths.app_path,
        paths.zip_path,
        paths.pkg_path,
    )
    .map_err(|error| {
        format!(
            "signed-distribution provenance report referenced by release evidence bundle is invalid: {error}"
        )
    })?;
    let live_qa =
        read_release_evidence_child_report(paths.live_qa_report, "live-device QA report")?;
    validate_live_device_qa_report(&live_qa).map_err(|error| {
        format!("live-device QA report referenced by release evidence bundle is invalid: {error}")
    })?;
    validate_live_device_signed_provenance_binding(
        &live_qa,
        &signed_provenance,
        paths.signed_provenance_report,
    )
    .map_err(|error| {
        format!("live-device QA report referenced by release evidence bundle is invalid: {error}")
    })?;
    let plugin_qa =
        read_release_evidence_child_report(paths.plugin_qa_report, "plugin-trust QA report")?;
    validate_plugin_trust_qa_report(&plugin_qa).map_err(|error| {
        format!("plugin-trust QA report referenced by release evidence bundle is invalid: {error}")
    })?;
    let bundle_generated_at = require_utc_report_timestamp_not_future(value, "generated_at")?;
    let bundle_completed_at =
        require_utc_report_timestamp(value, "owner_recorded_release_evidence.completed_at")?;
    for (label, report) in [
        ("signed-distribution provenance report", &signed_provenance),
        ("live-device QA report", &live_qa),
        ("plugin-trust QA report", &plugin_qa),
    ] {
        let child_generated_at =
            require_utc_report_timestamp(report, "generated_at").map_err(|error| {
                format!("{label} referenced by release evidence bundle is invalid: {error}")
            })?;
        if child_generated_at > bundle_completed_at {
            return Err(format!(
                "{label} referenced by release evidence bundle was generated after owner_recorded_release_evidence.completed_at"
            ));
        }
        if bundle_completed_at > bundle_generated_at {
            return Err(
                "release evidence bundle owner_recorded_release_evidence.completed_at must be less than or equal to generated_at"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn read_release_evidence_child_report(
    path: &FsPath,
    label: &str,
) -> Result<serde_json::Value, String> {
    let contents = fs::read_to_string(path).map_err(|error| {
        format!("{label} referenced by release evidence bundle is not readable: {error}")
    })?;
    serde_json::from_str(&contents).map_err(|error| {
        format!("{label} referenced by release evidence bundle is invalid JSON: {error}")
    })
}

fn env_value_alias(primary: &str, alias: &str, default: &str) -> String {
    std::env::var(primary)
        .or_else(|_| std::env::var(alias))
        .unwrap_or_else(|_| default.to_string())
}

fn expected_live_qa_version() -> String {
    std::env::var("JARVIS_QA_EXPECTED_VERSION")
        .or_else(|_| std::env::var("JARVIS_EVIDENCE_VERSION"))
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn expected_release_evidence_version() -> String {
    std::env::var("JARVIS_EVIDENCE_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn expected_release_bundle_id() -> String {
    env_value_alias(
        "JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID",
        "JARVIS_QA_EXPECTED_BUNDLE_ID",
        "com.nobiletechnology.jarvis",
    )
}

fn require_json_string_value(
    value: &serde_json::Value,
    dotted_path: &str,
    expected: &str,
) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    if found == expected {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} mismatch: expected {expected}, got {found}"
        ))
    }
}

fn require_json_bool_value(
    value: &serde_json::Value,
    dotted_path: &str,
    expected: bool,
) -> Result<(), String> {
    let found = dotted_path
        .split('.')
        .try_fold(value, |current, key| current.get(key))
        .and_then(|found| found.as_bool())
        .ok_or_else(|| format!("JSON report is missing required boolean field: {dotted_path}"))?;
    if found == expected {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} must be {expected}, got {found}"
        ))
    }
}

fn require_json_nonempty_string_value(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    if found.trim().is_empty() {
        return Err(format!("JSON report {dotted_path} must be non-empty"));
    }
    Ok(())
}

fn require_json_release_reports_archive_uri_value(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<(), String> {
    require_json_durable_evidence_archive_uri_value(value, dotted_path)
}

fn require_json_durable_evidence_archive_uri_value(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    let trimmed = found.trim();
    if trimmed.is_empty() {
        return Err(format!("JSON report {dotted_path} must be non-empty"));
    }
    let Some((scheme, location)) = trimmed.split_once(':') else {
        return Err(format!(
            "JSON report {dotted_path} must be a URI with a scheme"
        ));
    };
    let valid_scheme = !scheme.is_empty()
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'));
    if !valid_scheme || location.trim().is_empty() {
        return Err(format!(
            "JSON report {dotted_path} must be a URI with an archive location"
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    for placeholder in [
        "self-test",
        "placeholder",
        "example",
        "fixture",
        "todo",
        "tbd",
        "replace-me",
        "changeme",
        "/tmp/",
        "/temp/",
    ] {
        if lower.contains(placeholder) {
            return Err(format!(
                "JSON report {dotted_path} must point to a durable release evidence archive, not a placeholder or self-test location"
            ));
        }
    }
    Ok(())
}

fn require_json_string_prefix_value(
    value: &serde_json::Value,
    dotted_path: &str,
    expected_prefix: &str,
) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    if found.starts_with(expected_prefix) {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} must start with {expected_prefix}"
        ))
    }
}

fn require_json_string_contains_value(
    value: &serde_json::Value,
    dotted_path: &str,
    expected_fragment: &str,
) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    if found.contains(expected_fragment) {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} must include {expected_fragment}"
        ))
    }
}

fn require_gatekeeper_accepted_value(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    let accepted = found.lines().map(str::trim).any(|line| {
        line == "accepted"
            || line
                .rsplit_once(':')
                .is_some_and(|(_, tail)| tail.trim() == "accepted")
    });
    if accepted {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} must include an exact Gatekeeper accepted result"
        ))
    }
}

fn require_json_uuid_value(value: &serde_json::Value, dotted_path: &str) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    let parsed = Uuid::parse_str(found.trim())
        .map_err(|_| format!("JSON report {dotted_path} must be a UUID"))?;
    if parsed.is_nil() {
        return Err(format!("JSON report {dotted_path} must not be a nil UUID"));
    }
    Ok(())
}

fn require_json_sha256_value(value: &serde_json::Value, dotted_path: &str) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    let is_sha256 = found.len() == 64 && found.bytes().all(|byte| byte.is_ascii_hexdigit());
    if is_sha256 {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} must be a SHA-256 hex digest"
        ))
    }
}

fn require_json_team_identifier_value(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    let valid = found.len() == 10
        && found
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(format!(
            "JSON report field must be a 10-character Apple team identifier: {dotted_path}"
        ))
    }
}

fn require_json_cdhash_value(value: &serde_json::Value, dotted_path: &str) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    let valid =
        (40..=64).contains(&found.len()) && found.bytes().all(|byte| byte.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        Err(format!(
            "JSON report field must be a 40-64 character hexadecimal CDHash: {dotted_path}"
        ))
    }
}

fn require_utc_report_timestamp(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<DateTime<Utc>, String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    if !found.ends_with('Z') {
        return Err(format!(
            "JSON report {dotted_path} must be a UTC RFC3339 timestamp ending in Z"
        ));
    }
    DateTime::parse_from_rfc3339(&found)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|_| format!("JSON report {dotted_path} must be a UTC RFC3339 timestamp"))
}

fn require_utc_report_timestamp_not_future(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<DateTime<Utc>, String> {
    let timestamp = require_utc_report_timestamp(value, dotted_path)?;
    if timestamp > Utc::now() {
        return Err(format!(
            "JSON report {dotted_path} must not be later than the current time"
        ));
    }
    Ok(timestamp)
}

fn json_string_at(value: &serde_json::Value, dotted_path: &str) -> Option<String> {
    dotted_path
        .split('.')
        .try_fold(value, |current, key| current.get(key))
        .and_then(|found| found.as_str())
        .map(ToString::to_string)
}

fn release_json_present_detail(key: &str) -> String {
    match key {
        "release_evidence_bundle" => "JSON report exists, schema/evidence identity is valid, expected release version matches, artifact/report paths and SHA-256 digests match current artifacts and reports, owner evidence notes are non-placeholder, reports archive URI is durable and non-placeholder, and local signature validation is true; clean-profile, live-device, and plugin-trust claims remain owner-recorded external evidence".to_string(),
        "signed_distribution_provenance_report" => "JSON report exists, expected release version, bundle identifier, bundled core path/version/digest match, Apple-tool-derived signing/notarization/stapling/Gatekeeper evidence is semantically valid, required flags are true, and artifact SHA-256 digests match the current zip/pkg/core files; clean-profile install and live-device QA remain separate manual gates".to_string(),
        "live_device_qa_report" => "JSON report exists, required owner-recorded fields and proof boundary are non-empty, owner evidence notes are non-placeholder, installed app path, release metadata, bundled core executable path/version/SHA-256 binding, exact app-executable digest/code identity and signed-provenance path/SHA-256 binding, timestamps, observed transcript, observed command text, and task/audit command evidence reference match expected values; live-device claims are still owner-recorded external evidence".to_string(),
        "plugin_trust_qa_report" => "JSON report exists, schema/evidence identity is valid, expected release version matches, self-test fixture identity is false, required owner-recorded fields are present, owner evidence notes are non-placeholder, review and egress validation timestamps are valid and ordered, and deny/allow egress fixture notes are present; marketplace, malware, sandbox, and host-level egress claims remain owner-recorded external evidence".to_string(),
        _ => "JSON report exists and required owner-recorded fields are present; external claims are not revalidated by evidence-status".to_string(),
    }
}

fn json_field_is_present(value: &serde_json::Value, dotted_path: &str) -> bool {
    let Some(found) = dotted_path
        .split('.')
        .try_fold(value, |current, key| current.get(key))
    else {
        return false;
    };

    match found {
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::String(value) => !value.trim().is_empty(),
        serde_json::Value::Number(_) => true,
        serde_json::Value::Array(values) => !values.is_empty(),
        serde_json::Value::Object(values) => !values.is_empty(),
        serde_json::Value::Null => false,
    }
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(metadata: &fs::Metadata) -> bool {
    metadata.is_file()
}

fn release_blocking_manual_gates(
    evidence_status: &ReleaseEvidenceStatusResponse,
    evidence_mode_enabled: bool,
) -> Vec<String> {
    if evidence_mode_enabled && release_required_evidence_complete(evidence_status) {
        return Vec::new();
    }

    let live_device_qa_valid =
        release_live_device_evidence_valid(evidence_status, evidence_mode_enabled);
    let mut gates = vec![
        "Developer ID Application and Installer signing credentials configured and used for a full signed package run".to_string(),
        "notarization and stapling completed for both app and installer package".to_string(),
        "clean-profile installer run into /Applications".to_string(),
        "Finder/LaunchServices launch validation for the installed app".to_string(),
        "live microphone and Speech permission prompt validation plus spoken transcript handoff on a real Mac".to_string(),
        "live audio-output playback validation on a real Mac".to_string(),
        "manual clean-profile release QA pass covering installed-app command, audit, memory, scheduler, plugin, pause, diagnostics, restart behavior, and user-visible prompts".to_string(),
        "broader installed-plugin marketplace trust, malware analysis, and OS-level sandbox/egress enforcement before marketplace claims".to_string(),
        "final release evidence bundle generated and archived after signed distribution, live-device QA, and plugin-trust QA reports exist".to_string(),
    ];
    if live_device_qa_valid {
        gates.retain(|gate| {
            !gate.contains("clean-profile installer run")
                && !gate.contains("Finder/LaunchServices launch")
                && !gate.contains("live microphone")
                && !gate.contains("live audio-output")
                && !gate.contains("manual clean-profile release QA pass")
        });
    }
    gates
}

fn release_verification_commands() -> Vec<String> {
    vec![
        "./scripts/release-local.sh".to_string(),
        "./scripts/release-ci-workflow-smoke.sh".to_string(),
        "./scripts/release-operator-qa-smoke.sh".to_string(),
        "./scripts/package-distribution.sh --check".to_string(),
        "./scripts/package-distribution.sh --unsigned-launch-check".to_string(),
        "cargo run -p jarvis-cli -- release signed-distribution-runbook".to_string(),
        "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh".to_string(),
        "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_APPLE_ID='apple-id@example.com' JARVIS_NOTARYTOOL_TEAM_ID='TEAMID1234' JARVIS_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh".to_string(),
        "./scripts/release-external-handoff.sh --write target/release-external-handoff".to_string(),
        "cargo run -p jarvis-cli -- release live-device-runbook".to_string(),
        "./scripts/release-live-device-qa.sh --check".to_string(),
        "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env".to_string(),
        "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete".to_string(),
        "JARVIS_QA_CLEAN_PROFILE_VALIDATED=true JARVIS_QA_FINDER_LAUNCH_VALIDATED=true JARVIS_QA_MICROPHONE_VALIDATED=true JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true JARVIS_QA_NOTIFICATION_VALIDATED=true JARVIS_QA_RESTART_VALIDATED=true JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true JARVIS_QA_OWNER_NAME='Release Operator' JARVIS_QA_DEVICE_LABEL='Clean-profile release Mac' JARVIS_QA_PROFILE_LABEL='Clean macOS QA profile' JARVIS_QA_VOICE_CHECK_STARTED_AT='2026-05-22T16:00:00Z' JARVIS_QA_VOICE_CHECK_COMPLETED_AT='2026-05-22T16:05:00Z' JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE='Clean profile install observed' JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE='Finder launch observed' JARVIS_QA_MICROPHONE_EVIDENCE_NOTE='Microphone prompt and capture observed' JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE='Speech prompt and recognition observed' JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE='Spoken transcript reached the command path' JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE='Speech output playback observed' JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE='Scheduler notification observed' JARVIS_QA_NOTIFICATION_KIND='due_now' JARVIS_QA_NOTIFICATION_TITLE='Scheduler job ready: release verification' JARVIS_QA_NOTIFICATION_BODY='A scheduled Assemblywright job is due now.' JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER='jarvis.scheduler' JARVIS_QA_NOTIFICATION_OBSERVED_AT='2026-05-22T16:04:00Z' JARVIS_QA_RESTART_EVIDENCE_NOTE='Restart recovery observed' JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE='Manual release QA surfaces observed' JARVIS_QA_VOICE_TEST_PHRASE='Assemblywright status check' JARVIS_QA_OBSERVED_TRANSCRIPT='Assemblywright status check' JARVIS_QA_EXPECTED_COMMAND_TEXT='status check' JARVIS_QA_OBSERVED_COMMAND_TEXT='status check' JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid-from-live-command>' JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL='Built-in speakers' ./scripts/release-live-device-qa.sh --assert-complete".to_string(),
        "cargo run -p jarvis-cli -- release plugin-trust-runbook".to_string(),
        "./scripts/release-plugin-trust-qa.sh --check".to_string(),
        "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env".to_string(),
        "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete".to_string(),
        "JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=true JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED=true JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED=true JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED=true JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED=true JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED=true JARVIS_PLUGIN_QA_OWNER_NAME='Release Operator' JARVIS_PLUGIN_QA_REVIEW_STARTED_AT='2026-05-22T16:10:00Z' JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT='2026-05-22T16:20:00Z' JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE='Marketplace review evidence archived' JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_URI='archive://jarvis/plugin-trust/marketplace-review.json' JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_SHA256='1111111111111111111111111111111111111111111111111111111111111111' JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE='Malware scan evidence archived' JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_URI='archive://jarvis/plugin-trust/malware-scan.json' JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_SHA256='2222222222222222222222222222222222222222222222222222222222222222' JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE='OS sandbox validation evidence archived' JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_URI='archive://jarvis/plugin-trust/os-sandbox.json' JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_SHA256='3333333333333333333333333333333333333333333333333333333333333333' JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE='Host-level egress validation evidence archived' JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_URI='archive://jarvis/plugin-trust/egress.json' JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_SHA256='4444444444444444444444444444444444444444444444444444444444444444' JARVIS_PLUGIN_QA_EGRESS_POLICY_LABEL='Host egress policy/profile reviewed' JARVIS_PLUGIN_QA_EGRESS_VALIDATION_COMPLETED_AT='2026-05-22T16:18:00Z' JARVIS_PLUGIN_QA_EGRESS_DENY_FIXTURE_EVIDENCE_NOTE='Undeclared-host deny fixture evidence archived' JARVIS_PLUGIN_QA_EGRESS_ALLOW_FIXTURE_EVIDENCE_NOTE='Declared-host allow fixture evidence archived' JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE='Signed publisher policy evidence archived' JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_URI='archive://jarvis/plugin-trust/signed-publisher.json' JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_SHA256='5555555555555555555555555555555555555555555555555555555555555555' JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE='Manual plugin trust review evidence archived' JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_URI='archive://jarvis/plugin-trust/manual-review.json' JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_SHA256='6666666666666666666666666666666666666666666666666666666666666666' ./scripts/release-plugin-trust-qa.sh --assert-complete".to_string(),
        "cargo run -p jarvis-cli -- release evidence-bundle-runbook".to_string(),
        "./scripts/release-evidence-bundle.sh --check".to_string(),
        "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env".to_string(),
        "./scripts/release-evidence-doctor.sh --check".to_string(),
        "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle".to_string(),
        "JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true JARVIS_EVIDENCE_REPORTS_ARCHIVED=true JARVIS_EVIDENCE_OWNER_NAME='Release Operator' JARVIS_EVIDENCE_COMPLETED_AT='2026-05-22T17:00:00Z' JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_NOTE='Signed distribution evidence archived' JARVIS_EVIDENCE_NOTARIZATION_NOTE='Notarization evidence archived' JARVIS_EVIDENCE_CLEAN_PROFILE_NOTE='Clean-profile evidence archived' JARVIS_EVIDENCE_LIVE_DEVICE_QA_NOTE='Live-device QA evidence archived' JARVIS_EVIDENCE_PLUGIN_TRUST_QA_NOTE='Plugin-trust QA evidence archived' JARVIS_EVIDENCE_REPORTS_ARCHIVE_NOTE='Release reports archived' JARVIS_EVIDENCE_REPORTS_ARCHIVE_URI='<archive-uri>' ./scripts/release-evidence-bundle.sh --bundle".to_string(),
        "./scripts/release-evidence-doctor.sh --assert-complete".to_string(),
        "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external".to_string(),
        "cargo run -p jarvis-cli -- release readiness".to_string(),
    ]
}

fn contract_features() -> Vec<ContractFeature> {
    vec![
        feature(
            "app_supervised_ipc_auth",
            "implemented",
            "Default app supervision rotates a 32-byte bearer through strict bounded startup stdin and uses a generation-random owner-only Unix socket. Swift and Rust obtain LOCAL_PEERTOKEN, validate the running peer against the launch-supplied Security.framework designated requirement and current EUID before framing, require half-close without trailing input, and protect the whole router with constant-time bearer comparison. The release-built smoke proves legitimate audit-token requirement acceptance and same-EUID wrong-code pre-frame rejection. Exact CLI handoff opt-in replaces UDS with weaker authenticated loopback TCP plus a bounded owner-only token file.",
            "Ad-hoc exact-build requirements bind the evaluated cdhash but do not prove Developer ID publisher identity. The Developer ID hardened profile still requires signed/notarized clean-profile evidence. This defense in depth is not device authentication, same-user/process isolation, XPC, App Sandbox enforcement, host-level egress policy, notarization, or live-device proof. Explicit CLI handoff is same-user-readable loopback compatibility; explicitly launched legacy servers remain unauthenticated but reject any Authorization header.",
        ),
        feature(
            "repository_state",
            "implemented",
            "SQLite-backed task, audit, model-route, memory, scheduler, approval, and installed-plugin state is covered by Rust unit tests and local IPC E2E.",
            "Local repository evidence only; no hosted sync or multi-device state claim.",
        ),
        feature(
            "active_command_cancellation",
            "implemented",
            "An optional client-generated UUID cancellation_id is registered for the full active POST /commands lifetime. The Swift console serializes submissions before changing its active handle. Authenticated POST /runtime/cancellations/:id targets only that handle, propagates to its bound task and active provider/tool work, and the final guard suppresses late model steps and tool results when cancellation wins. Rust race/unit tests, real-server CLI IPC E2E, and Swift model tests cover the boundary.",
            "The registry is process-local, capped at 128 active handles, and retains the 1,024 most recently consumed UUIDs as FIFO tombstones so delayed stale cancellation cannot target a new run that reuses a recent handle. Clients must always generate fresh random UUIDs; a tombstone can be evicted after the bounded window or lost on process restart. cancellation_requested proves an active local command accepted the signal; not_found means no active execution existed at that linearization point. Cancellation cannot reverse an external effect that already occurred and is not distributed cancellation or crash recovery.",
        ),
        feature(
            "activity_events",
            "implemented",
            "Repository-backed `/activity/events` exposes bounded redacted task metadata, audit event batches, redacted installed-plugin progress, model-step progress, and model-output chunk metadata frames and is covered by CLI IPC E2E plus Swift decoding tests.",
            "This is bounded state polling over SSE from completed audit evidence; activity recent tasks omit command bodies, model-output chunks expose counts with content_redacted:true rather than raw token text, and the Swift client buffers each bounded watch response rather than rendering live tokens.",
        ),
        feature(
            "ollama_native_transport_streaming",
            "implemented",
            "The Ollama adapter requests native NDJSON streaming, enforces byte/response/metadata limits and a terminal done frame, supports in-flight runtime cancellation, then parses the quarantined final response before any audit or tool-plan exposure.",
            "Transport progress metadata only after terminal validation; no partial raw text or tool envelope reaches IPC, Swift transcript, audit, or execution, and this is not raw-token UI streaming or production-readiness proof.",
        ),
        feature(
            "scheduler_attention",
            "implemented",
            "Repository-backed scheduler jobs expose redacted `/scheduler/attention`, plus a schema-v14 bounded durable occurrence outbox with compare-and-swap acknowledgement after app submission or explicit no-authorization suppression. Due occurrence claim precedes execution, failed and stale-running outcomes revision-escalate atomically, and app notification identifiers are stable per occurrence revision.",
            "This is an at-least-once app-notification handoff: concurrent consumers or a crash after notification-center submission but before acknowledgement may repeat a stable request. It does not prove live OS display, background OS wake, or proactive plugin authorization, and live-device notification evidence remains a manual release gate.",
        ),
        feature(
            "trusted_macos_system_wake",
            "implemented",
            "A disabled-by-default schema-v11 wake rule accepts bounded P-256 envelopes, supports old-key-signed rotation plus explicit stronger-warning lost-key recovery through short-lived one-shot grants, persists replay/dispatch evidence, and enters the existing proactive policy funnel.",
            "Local enrolled-key possession and explicit key control only. The packaged app requires per-launch bearer possession while an explicit legacy server does not; recovery confirmation remains accident prevention, not device authentication, OS identity, ownership proof, or same-user/process isolation; no Apple attestation, OS wake provenance, background launch reliability, exactly-once side effects, live-device QA, or production readiness is claimed.",
        ),
        feature(
            "scheduler_trigger_policy_review",
            "implemented",
            "Active scheduler triggers appear in `/permissions/policy-review` without scheduler command text; due execution emits `scheduler_proactive_policy_checked` using the same trigger classification, and proactive plugin call requests require manifest opt-in plus `proactive_run` permission.",
            "Review visibility only for scheduler policy review; due-run audit and plugin opt-in enforcement are local-only, scheduler command bodies remain redacted, proactive plugin requests that are not opted in fail closed, and live OS notification delivery remains a manual release gate.",
        ),
        feature(
            "scheduler_stale_running_recovery",
            "implemented",
            "Explicit `/scheduler/recover-stale` plus opt-in startup recovery mark stale running jobs failed with redacted audit evidence and are covered by Rust unit plus CLI IPC E2E tests.",
            "Bounded local stale-job cleanup only; no default background recovery or distributed lease claim.",
        ),
        feature(
            "memory_policy_review",
            "implemented",
            "Unreviewed memory items and deleted sensitive retained memory appear in `/permissions/policy-review` with redacted values; `/memory/retention-plan` exposes the memory-specific redacted operator action queue; diagnostics export exposes only aggregate memory review counts.",
            "Review visibility and retention-action planning only; no autonomous memory rewrite or purge automation claim.",
        ),
        feature(
            "memory_index_governance",
            "implemented",
            "Versioned local memory-index manifests are atomically rebuilt from canonical active SQLite memory records; redacted status reports current, missing, stale, deleted, orphaned, and corrupt projection counts with Rust, CLI IPC E2E, and Swift coverage.",
            "SQLite remains canonical and the projection is a local rebuildable eligibility gate, not a source of memory values, cloud context, or autonomous rewriting.",
        ),
        feature(
            "bounded_local_memory_retrieval",
            "implemented",
            "Explicit CLI and Swift opt-in can attach deterministic lexical context to a selected local non-proactive route. Retrieval requires a current index, reviewed active Public/Workspace/Personal records, strict query/item/corpus/result/context caps, pause/cancel checks, untrusted-data framing, redacted audit counts, and is covered by Rust unit, cross-process CLI/Ollama-stub, and Swift tests.",
            "Disabled by default and local-model-only. Private, CredentialAdjacent, Restricted, unreviewed, deleted, missing/stale/corrupt, proactive, cloud, and over-budget paths fail closed. This is not vector/embedding search, automatic retrieval, autonomous memory rewrite/purge, or production relevance proof.",
        ),
        feature(
            "approval_execution",
            "implemented",
            "Approved first-party and installed-plugin actions execute through `/approvals/:id/execute` only after matching approval_granted audit evidence plus current action, risk, scope, input-schema, and policy validation; schema-v15 installed bindings additionally protect canonical input, manifest/provenance, and execution grant. Missing, unrelated, or changed evidence fails before claim or plugin entry; current redacted and exact legacy raw-metadata audit shapes are accepted only when their authority and decision fields match. Schema-v13 atomically records a unique durable execution claim plus redacted policy/claim audits before plugin invocation; terminal state, task state, and terminal audits commit together. Schema-v16 startup reconciliation inserts durable redacted attention once for unresolved claimed executions, safely fails a still-waiting task, and exposes exact-revision acknowledgement_without_retry through authenticated IPC, CLI, and Swift without entering plugin runtime or altering the claim. Rust migration/CAS/redaction tests, authenticated cross-process restart E2E, and Swift model/request tests cover the boundary.",
            "Every durable claim permanently consumes that approval. Failure, cancellation, timeout, storage interruption, or restart after claim may leave the effect ambiguous and automatic retry is forbidden; acknowledgement records operator review only and cannot prove whether a pre-restart effect occurred. A deliberate new attempt requires a new approval. Grant/deny remains side-effect-free, the claim path never fabricates grant evidence, and this is not broad autonomous execution or distributed exactly-once delivery.",
        ),
        feature(
            "model_tool_catalog_grounding",
            "implemented",
            "`/tools/model` exposes the redacted default first-party model-tool catalog, Ollama prompts use the same per-request JSON allowlist, ChatGPT/OpenAI-compatible tool schemas are derived from that allowlist, and invalid model-planned plugin IDs/actions are rejected before policy or execution with registered-tool audit guidance and CLI IPC E2E coverage. Eligible installed local_wasm actions are added only to an explicitly opted-in local reactive command.",
            "The default catalog remains first-party only. Installed subprocess, network, model, memory, high-risk, proactive, disabled, stale, or provenance-mismatched capabilities are excluded, and this is not broad third-party tool execution, marketplace trust, malware analysis, or OS-level sandboxing.",
        ),
        feature(
            "model_planned_installed_wasm_tools",
            "implemented",
            "An additive installed_wasm_tools request flag, false when absent, can advertise eligible installed local_wasm compute actions only to a selected local non-proactive model route. Eligibility requires enabled execution, current exact artifact provenance, wasm_compute grant, low-risk non-proactive actions, no permissions, memory, model, or network capability, valid schemas, and the same bounded direct WASM runner with immediate pre-execution revalidation, cancellation, emergency-pause dominance, and redacted audit evidence.",
            "Explicit opt-in is scoped to one reactive command and does not include subprocess plugins or cloud routes. Wasmi provides import-free language-level compute confinement with bounded fuel, memory, request, and output, but not an OS sandbox, publisher/marketplace trust, malware analysis, same-user/process isolation, host-level egress enforcement, signing/notarization, or live-device production evidence.",
        ),
        feature(
            "installed_plugin_execution",
            "implemented",
            "Local subprocess plugins retain explicit source-matched grants and bounded JSON streams with os_sandbox_enforced:false audit evidence. Compute-only local_wasm plugins require wasm_compute, exact current artifact provenance, no imports/WASI/filesystem/network/environment, fixed module/request/output/memory/fuel limits, and pause-dominant resumable execution. Repository locks are released before either untrusted runtime starts and reacquired only for redacted audit persistence.",
            "Wasmi supplies language-level confinement for low-risk compute-only modules, not an OS process sandbox, marketplace approval, malware analysis, publisher reputation, same-user/process IPC isolation, or host-level egress proof. Legacy subprocess audit remains os_sandbox_enforced:false.",
        ),
        feature(
            "wasm_plugin_confinement",
            "implemented",
            "The jarvis_json_v1 ABI requires memory, jarvis_alloc(i32)->i32, and jarvis_run(i32,i32)->i64 exports; every import is rejected and exact executed bytes are bound to install provenance. Unit and cross-process tests cover success plus import, mutation, schema, resource, timeout, pause, and restart denial paths.",
            "Compute-only Wasmi boundary with no host capabilities. It does not clear external plugin-trust, signed-distribution, or live-device evidence gates.",
        ),
        feature(
            "plugin_publisher_signature",
            "implemented",
            "Installed plugin manifests can verify an Ed25519 publisher signature against an explicit trusted public key with audit evidence.",
            "Trusted-key verification only; not marketplace approval, malware analysis, or reputation service trust.",
        ),
        feature(
            "plugin_network_governance",
            "implemented",
            "Network-capable plugin actions must declare exact allowed hosts, appear in permission policy review, and require the explicit subprocess_stdio_network execution grant.",
            "Runtime grant gate plus manifest governance only; not OS-level network sandbox enforcement or host-level egress filtering.",
        ),
        feature(
            "operator_release_qa_smoke",
            "implemented",
            "`release-operator-qa-smoke.sh` exercises repository-backed command, audit, route, memory, scheduler, activity, permission, diagnostics, pause, readiness, and restart paths in one local QA lane.",
            "Local CLI/operator QA evidence only; not clean-profile installed-app QA, Finder/LaunchServices validation, live voice/audio validation, live notification delivery, notarization, or marketplace trust.",
        ),
        feature(
            "release_ci_gate",
            "implemented",
            "`.github/workflows/release-local.yml` runs `./scripts/release-local.sh` on macOS for pull requests, pushes to main, and manual dispatch; `release-ci-workflow-smoke.sh` is part of the local gate and verifies the workflow remains wired to the canonical release script.",
            "Public CI evidence for the repo-owned local release gate only; it does not perform Developer ID signing, notarization, clean-profile installation, Finder/LaunchServices validation, live-device QA, plugin-trust QA, malware review, or OS sandbox enforcement.",
        ),
        feature(
            "unsigned_distribution_launch",
            "implemented",
            "`package-distribution.sh --unsigned-launch-check` builds the release app layout, creates an unsigned installer payload, launches the release-built app executable with isolated HOME, proves the default owner-only Unix socket plus memory-only bearer path has no TCP listener or credential handoff file, validates audit-token requirements and same-EUID wrong-code pre-frame rejection, then relaunches only for the explicit authenticated TCP/token CLI compatibility check.",
            "Unsigned distribution-layout, ad-hoc exact-build identity mechanics, and bearer-possession proof only; not Developer ID publisher identity, device authentication, App Sandbox or host-egress enforcement, signing, notarization, stapling, /Applications install, Finder/LaunchServices validation, live-device validation, App Store review, or manual QA.",
        ),
        feature(
            "release_evidence_status",
            "implemented",
            "`/release/evidence-status` and `assemblywright release evidence-status` expose structured present, missing, or invalid status for standard signed artifacts, QA reports, and final evidence bundle paths, including app bundle metadata and approved privacy prompt copy matching, bundled core version-marker matching, signed-provenance core path/version/digest binding, signed-provenance artifact digest matching, live-device QA bundle/version/non-future timestamp and repository-backed command-result evidence checks, plugin-trust release-version, non-future timestamp, owner-review-source, host-egress policy and deny/allow fixture checks, and final-bundle path/digest/local-signature/archive-URI validation plus child-report semantic revalidation, with Rust, CLI E2E, and Swift model coverage.",
            "Read-only file/report inventory plus report semantic validation only; it does not sign, notarize, staple, install, Finder-launch, execute release artifacts, run live-device QA, review marketplace trust, scan malware, or enforce OS sandboxing.",
        ),
        feature(
            "release_evidence_bundle",
            "implemented",
            "`release-evidence-bundle.sh --check`, `--write-template`, `--self-test`, and `release-evidence-doctor.sh --check` are part of the release evidence workflow; `--bundle` validates signed/stapled artifact references, live-device QA bundle metadata and command observation, plugin-trust QA flags, owner evidence, review source, host-egress fields, and a durable reports archive URI, then writes SHA-256-bound evidence manifest entries whose child reports are revalidated by doctor/status checks.",
            "Evidence-bundle mechanics, local artifact/report validation, and release-evidence inventory only; production readiness still depends on owner-recorded external signing, notarization, live-device QA, plugin-trust QA, and archived evidence.",
        ),
        feature(
            "live_voice_loop",
            "pending_manual_validation",
            "Swift voice input and speech-output adapters have deterministic fake-adapter tests, including final transcript staging and opt-in final-transcript auto-submit into the text command path.",
            "Live microphone, Speech permission, spoken transcript handoff, live audio output, and device validation are not proven by automated tests.",
        ),
    ]
}

fn feature(
    key: impl Into<String>,
    status: impl Into<String>,
    proof: impl Into<String>,
    boundary: impl Into<String>,
) -> ContractFeature {
    ContractFeature {
        key: key.into(),
        status: status.into(),
        proof: proof.into(),
        boundary: boundary.into(),
    }
}

/// Conservative release-readiness summary derived from feature metadata,
/// checklist blockers, and explicitly enabled evidence status.
pub fn release_readiness() -> ReleaseReadinessResponse {
    let evidence_status = release_evidence_status();
    let evidence_mode_enabled = release_readiness_evidence_mode_enabled();
    let features = release_readiness_features(&evidence_status, evidence_mode_enabled);
    let implemented_features = features
        .iter()
        .filter(|feature| feature.status == "implemented")
        .cloned()
        .map(ReleaseReadinessFeature::from)
        .collect::<Vec<_>>();
    let pending_features = features
        .into_iter()
        .filter(|feature| feature.status != "implemented")
        .map(ReleaseReadinessFeature::from)
        .collect::<Vec<_>>();
    let production_ready = release_production_ready(
        &evidence_status,
        evidence_mode_enabled,
        pending_features.is_empty(),
    );

    ReleaseReadinessResponse {
        generated_at: Utc::now(),
        production_ready,
        evidence_mode_enabled,
        readiness_scope:
            "local Rust foundation and Swift shell evidence plus explicitly enabled external release evidence status"
                .to_string(),
        verified_feature_count: implemented_features.len(),
        pending_feature_count: pending_features.len(),
        implemented_features,
        pending_features,
        blocking_manual_gates: release_blocking_manual_gates(&evidence_status, evidence_mode_enabled),
        recommended_verification_commands: release_verification_commands(),
        proof_boundary:
            "Read-only summary derived from repository feature metadata, release checklist blockers, and explicitly enabled release evidence status; it does not perform signing, notarization, stapling, installation, Finder/LaunchServices validation, or live-device validation."
                .to_string(),
    }
}

/// Read-only inventory and structural validation of release evidence paths and
/// owner-recorded reports.
pub fn release_evidence_status() -> ReleaseEvidenceStatusResponse {
    release_evidence_status_from_env()
}

pub fn release_live_device_runbook() -> ReleaseRunbookResponse {
    release_live_device_runbook_from(&release_readiness(), &release_evidence_status())
}

pub fn release_signed_distribution_runbook() -> ReleaseRunbookResponse {
    release_signed_distribution_runbook_from(&release_readiness(), &release_evidence_status())
}

pub fn release_plugin_trust_runbook() -> ReleaseRunbookResponse {
    release_plugin_trust_runbook_from(&release_readiness(), &release_evidence_status())
}

pub fn release_evidence_bundle_runbook() -> ReleaseRunbookResponse {
    release_evidence_bundle_runbook_from(&release_readiness(), &release_evidence_status())
}
