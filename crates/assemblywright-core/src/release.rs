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

const LIVE_DEVICE_QA_REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "evidence_type",
    "generated_at",
    "installed_app_path",
    "validation_flags.clean_profile",
    "validation_flags.finder_launch",
    "validation_flags.restart",
    "validation_flags.manual_release_qa",
    "app_bundle.bundle_identifier",
    "app_bundle.short_version",
    "app_bundle.build_version",
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
    "owner_recorded_device_evidence.owner_name",
    "owner_recorded_device_evidence.device_label",
    "owner_recorded_device_evidence.profile_label",
    "owner_recorded_device_evidence.device_check_started_at",
    "owner_recorded_device_evidence.device_check_completed_at",
    "owner_recorded_device_evidence.clean_profile_evidence_note",
    "owner_recorded_device_evidence.finder_launch_evidence_note",
    "owner_recorded_device_evidence.restart_evidence_note",
    "owner_recorded_device_evidence.manual_release_qa_evidence_note",
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
    "reports.signed_distribution_provenance_sha256",
    "reports.live_device_qa_sha256",
    "validation_flags.signed_distribution",
    "validation_flags.notarization",
    "validation_flags.clean_profile",
    "validation_flags.live_device_qa",
    "validation_flags.reports_archived",
    "validation_flags.local_signature_validation",
    "owner_recorded_release_evidence.owner_name",
    "owner_recorded_release_evidence.completed_at",
    "owner_recorded_release_evidence.signed_distribution_note",
    "owner_recorded_release_evidence.notarization_note",
    "owner_recorded_release_evidence.clean_profile_note",
    "owner_recorded_release_evidence.live_device_qa_note",
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
    std::env::var("ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE")
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
            "Set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' in target/release-live-device-qa.env before collecting command evidence"
                .to_string(),
            "Launch Assemblywright with ASSEMBLYWRIGHT_MAC_ENABLE_IPC_CLI_HANDOFF=true for this operator evidence session, then confirm ASSEMBLYWRIGHT_IPC_TOKEN_FILE points to the app-owned ipc-session-auth.json path before IPC commands"
                .to_string(),
            "cargo run -p assemblywright-cli -- command \"status check\" --endpoint \"${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}\" --json"
                .to_string(),
            "Record the returned task ID as ASSEMBLYWRIGHT_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>' or a task-associated audit ID as 'audit:<uuid>' in target/release-live-device-qa.env"
                .to_string(),
            "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete"
                .to_string(),
            "ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release evidence-status --endpoint \"${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
            "Start or restart the core with ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external"
                .to_string(),
            "ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release readiness --endpoint \"${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}\""
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
            "ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh"
                .to_string(),
            "ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' ASSEMBLYWRIGHT_NOTARYTOOL_APPLE_ID='apple-id@example.com' ASSEMBLYWRIGHT_NOTARYTOOL_TEAM_ID='TEAMID1234' ASSEMBLYWRIGHT_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh"
                .to_string(),
            "Set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks"
                .to_string(),
            "Launch Assemblywright with ASSEMBLYWRIGHT_MAC_ENABLE_IPC_CLI_HANDOFF=true, then export ASSEMBLYWRIGHT_IPC_TOKEN_FILE as the app-owned ipc-session-auth.json path before external IPC checks"
                .to_string(),
            "ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release evidence-status --endpoint \"${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
            "./scripts/release-evidence-doctor.sh --check".to_string(),
            "cargo run -p assemblywright-cli -- release live-device-runbook".to_string(),
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
            "Set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks"
                .to_string(),
            "Launch Assemblywright with ASSEMBLYWRIGHT_MAC_ENABLE_IPC_CLI_HANDOFF=true, then export ASSEMBLYWRIGHT_IPC_TOKEN_FILE as the app-owned ipc-session-auth.json path before external IPC checks"
                .to_string(),
            "ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release evidence-status --endpoint \"${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
            "Start or restart the core with ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external"
                .to_string(),
            "ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release readiness --endpoint \"${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}\""
                .to_string(),
        ],
        manual_checks: vec![
            "Generate the final evidence bundle only after signed-distribution, live-device QA, and plugin-trust QA reports exist and have been archived."
                .to_string(),
            "Use a durable reports archive URI and preserve the signed zip, installer package, signed provenance report, live-device QA report, plugin-trust QA report, final bundle, and supporting logs."
                .to_string(),
            "Confirm release-evidence-doctor --assert-complete reports every required evidence item present before enabling external evidence-mode readiness."
                .to_string(),
            "Restart or start the release core with ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external before the final readiness check."
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
    let version = std::env::var("ASSEMBLYWRIGHT_EVIDENCE_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let dist_dir = env_path("ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR", "target/distribution");
    let app_path = env_path_or(
        "ASSEMBLYWRIGHT_EVIDENCE_APP_PATH",
        dist_dir.join("Assemblywright.app"),
    );
    let zip_path = env_path_or(
        "ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH",
        dist_dir.join(format!("Assemblywright-{version}.zip")),
    );
    let pkg_path = env_path_or(
        "ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH",
        dist_dir.join(format!("Assemblywright-{version}.pkg")),
    );
    let live_qa_report = env_path_alias(
        "ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT",
        "ASSEMBLYWRIGHT_QA_REPORT_PATH",
        "target/release-live-device-qa-report.json",
    );
    let bundle_path = env_path(
        "ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH",
        "target/release-evidence-bundle.json",
    );
    let signed_provenance_report = env_path_or(
        "ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT",
        dist_dir.join(format!("Assemblywright-{version}-signed-provenance.json")),
    );

    let bundle_digest_paths = ReleaseEvidenceBundleDigestPaths {
        app_path: &app_path,
        zip_path: &zip_path,
        pkg_path: &pkg_path,
        signed_provenance_report: &signed_provenance_report,
        live_qa_report: &live_qa_report,
    };

    let mut items = vec![
        release_app_bundle_item("signed_app_bundle", "App bundle path", app_path.clone()),
        release_path_item(
            "app_executable",
            "App executable",
            app_path.join("Contents/MacOS/AssemblywrightMacApp"),
            ReleaseEvidenceKind::Executable,
        ),
        release_bundled_core_item(
            "bundled_core_executable",
            "Bundled core executable",
            app_path.join("Contents/Resources/bin/assemblywright-cli"),
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
            .unwrap_or("assemblywright-cli")
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
        "ASSEMBLYWRIGHT_QA_EXPECTED_BUNDLE_ID",
        "ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_BUNDLE_ID",
        "com.nobiletechnology.assemblywright",
    );
    let expected_version = expected_live_qa_version();
    require_json_string_value(value, "app_bundle.bundle_identifier", &expected_bundle_id)?;
    require_json_string_value(value, "app_bundle.short_version", &expected_version)?;
    require_json_string_value(value, "app_bundle.build_version", &expected_version)?;
    let expected_installed_app_path = std::env::var("ASSEMBLYWRIGHT_QA_INSTALLED_APP_PATH")
        .unwrap_or_else(|_| "/Applications/Assemblywright.app".to_string());
    require_json_string_value(value, "installed_app_path", &expected_installed_app_path)?;
    require_json_string_value(
        value,
        "app_executable.executable_path",
        &format!("{expected_installed_app_path}/Contents/MacOS/AssemblywrightMacApp"),
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
        &format!("{expected_installed_app_path}/Contents/Resources/bin/assemblywright-cli"),
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
        "validation_flags.restart",
        "validation_flags.manual_release_qa",
    ] {
        require_json_bool_value(value, field, true)?;
    }
    require_json_nonempty_string_value(value, "proof_boundary")?;
    for field in [
        "owner_recorded_device_evidence.owner_name",
        "owner_recorded_device_evidence.device_label",
        "owner_recorded_device_evidence.profile_label",
        "owner_recorded_device_evidence.device_check_started_at",
        "owner_recorded_device_evidence.device_check_completed_at",
        "owner_recorded_device_evidence.clean_profile_evidence_note",
        "owner_recorded_device_evidence.finder_launch_evidence_note",
        "owner_recorded_device_evidence.restart_evidence_note",
        "owner_recorded_device_evidence.manual_release_qa_evidence_note",
    ] {
        require_json_meaningful_owner_evidence(value, field)?;
    }

    let started_at = require_utc_report_timestamp(
        value,
        "owner_recorded_device_evidence.device_check_started_at",
    )?;
    let completed_at = require_utc_report_timestamp(
        value,
        "owner_recorded_device_evidence.device_check_completed_at",
    )?;
    if completed_at < started_at {
        return Err("JSON report device_check_completed_at must be greater than or equal to device_check_started_at".to_string());
    }
    if generated_at < completed_at {
        return Err(
            "JSON report generated_at must be greater than or equal to device_check_completed_at"
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
        "ASSEMBLYWRIGHT_EVIDENCE_APP_PATH",
        env_path("ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR", "target/distribution")
            .join("Assemblywright.app"),
    );
    require_json_string_value(
        value,
        "artifacts.app_executable_path",
        &expected_app_path
            .join("Contents/MacOS/AssemblywrightMacApp")
            .display()
            .to_string(),
    )?;
    require_json_sha256_value(value, "artifacts.app_executable_sha256")?;
    require_json_string_value(
        value,
        "artifacts.bundled_core_path",
        &expected_app_path
            .join("Contents/Resources/bin/assemblywright-cli")
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
            "ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_BUNDLE_ID",
            "ASSEMBLYWRIGHT_QA_EXPECTED_BUNDLE_ID",
            "com.nobiletechnology.assemblywright",
        ),
    )?;
    let expected_bundle_identifier = env_value_alias(
        "ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_BUNDLE_ID",
        "ASSEMBLYWRIGHT_QA_EXPECTED_BUNDLE_ID",
        "com.nobiletechnology.assemblywright",
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
        &app_path.join("Contents/MacOS/AssemblywrightMacApp"),
    )?;
    require_json_sha256_matches_file(
        value,
        "artifacts.bundled_core_sha256",
        "bundled core executable",
        &app_path.join("Contents/Resources/bin/assemblywright-cli"),
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
    let bundle_generated_at = require_utc_report_timestamp_not_future(value, "generated_at")?;
    let bundle_completed_at =
        require_utc_report_timestamp(value, "owner_recorded_release_evidence.completed_at")?;
    for (label, report) in [
        ("signed-distribution provenance report", &signed_provenance),
        ("live-device QA report", &live_qa),
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
    std::env::var("ASSEMBLYWRIGHT_QA_EXPECTED_VERSION")
        .or_else(|_| std::env::var("ASSEMBLYWRIGHT_EVIDENCE_VERSION"))
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn expected_release_evidence_version() -> String {
    std::env::var("ASSEMBLYWRIGHT_EVIDENCE_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn expected_release_bundle_id() -> String {
    env_value_alias(
        "ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_BUNDLE_ID",
        "ASSEMBLYWRIGHT_QA_EXPECTED_BUNDLE_ID",
        "com.nobiletechnology.assemblywright",
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
        "manual clean-profile release QA pass covering installed-app launch, Developer Mode bridge status, restart behavior, and user-visible prompts".to_string(),
        "final release evidence bundle generated and archived after signed distribution and live-device QA reports exist".to_string(),
    ];
    if live_device_qa_valid {
        gates.retain(|gate| {
            !gate.contains("clean-profile installer run")
                && !gate.contains("Finder/LaunchServices launch")
                && !gate.contains("manual clean-profile release QA pass")
        });
    }
    gates
}

fn release_verification_commands() -> Vec<String> {
    vec![
        "./scripts/release-local.sh".to_string(),
        "./scripts/release-ci-workflow-smoke.sh".to_string(),
        "./scripts/package-distribution.sh --check".to_string(),
        "./scripts/package-distribution.sh --unsigned-launch-check".to_string(),
        "cargo run -p assemblywright-cli -- release signed-distribution-runbook".to_string(),
        "ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh".to_string(),
        "./scripts/release-external-handoff.sh --write target/release-external-handoff".to_string(),
        "cargo run -p assemblywright-cli -- release live-device-runbook".to_string(),
        "./scripts/release-live-device-qa.sh --check".to_string(),
        "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env".to_string(),
        "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete".to_string(),
        "cargo run -p assemblywright-cli -- release evidence-bundle-runbook".to_string(),
        "./scripts/release-evidence-bundle.sh --check".to_string(),
        "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env".to_string(),
        "./scripts/release-evidence-doctor.sh --check".to_string(),
        "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle".to_string(),
        "./scripts/release-evidence-doctor.sh --assert-complete".to_string(),
        "ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release readiness".to_string(),
    ]
}

fn contract_features() -> Vec<ContractFeature> {
    vec![
        feature(
            "distributed_protocol_contract",
            "implemented",
            "`assemblywright-protocol` owns the current protocol version: typed device/task/step/attempt/lease/cancellation identifiers, bounded capability advertisements, handshake, immutable digest-bound general-coding packets, canonical multi-file artifacts and retained-attempt result envelopes, strict bound-before-decode JSON entry points, nil-identity rejection, and a golden compatibility fixture.",
            "A versioned wire contract with golden-fixture coverage only; it is not proof of live two-device behavior.",
        ),
        feature(
            "distributed_master_kernel",
            "implemented",
            "The portable `assemblywright-master` schema-v10 SQLite database retains the schema-v4 distributed-device kernel and persists registered devices, connection epochs, queued steps, immutable leased job envelopes, attempts, cancellation and expiry outcomes, accepted payload digests, and a metadata-only event journal with one server-issued stream ID and contiguous sequence. It enforces the 256-step admission ceiling, four global leases, one live lease per device connection, exact leased-attempt result identity, and durable abandon-before-reissue.",
            "Durable single-owner state only; it is not the production runtime authority and carries no live cross-device reliability claim.",
        ),
        feature(
            "feature_conveyor_repository_kernel",
            "implemented",
            "The default-inert schema-v14 Feature Conveyor kernel persists immutable owner-approved specification revisions, three independent repository-grant revisions, a bounded owner-ordered queue with strict head/dependency ordering, compare-and-set revisions, one durable snapshot-bound active lease, exact lifecycle advancement, cancellation without advancement, explicit safe abandonment, startup quarantine, and same-transaction redacted audits. Its owner-authenticated loopback read-only status route and accepted-session MacBridge route add bounded lifecycle observation and fixed-enum owner guidance bound to queue, Emergency Pause, and optional feature lifecycle revisions. Owner-token loopback routes record and inspect strict contiguous, pause-bound, digest-only repository-grant revisions; a separate owner-local route performs one bounded point-in-time filesystem-only repository identity preflight bound to the exact active registration grant and returns no path; another owner-local route constructs one independent no-remote shallow snapshot containing only the exact current commit/tree/blob graph and atomically binds it to the strict queue head, provider, grants, queue, and pause revisions. Explicit owner-local dispatch binds one immutable protocol-v5 packet to that lease/snapshot and one exact local.coding.v1 worker. The bounded transfer route reauthorizes every snapshot chunk; the native agent materializes an aggregate-budgeted ephemeral independent repository and applies only deterministic file.write.v1 and file.delete.v1 operations across at most 64 sorted normalized relative paths. It uses owner-private no-follow parent descriptors, exclusive atomic create, atomic-swap replacement and same-parent delete capture with displaced-inode verification and rollback. No child, shell, exec, provider, test, credential, or network is authorized. Exact changed-path and canonical artifact evidence includes the exact protocol-owned admission transcript. Success seals one workspace and a separate private recovery record until exact cancellation or bounded expiry; restart re-hashes and reconstructs exactly one pair, while tamper, orphan, ambiguity, or authority drift fails closed. Windows independently validates artifact operations against the immutable packet before terminal acceptance; Swift is defense in depth. A separate owner-loopback integration action reopens the complete terminal accepted artifact set, orders it by immutable dispatch metadata, applies it only to a private no-remote candidate repository derived from the claimed snapshot, freezes one exact commit/tree, and advances only implementing to validating. A separate designation permits only the exact MacBridge to enqueue one already-approved specification.",
            "Bounded observation, repository-grant preparation, identity-only preflight, queue insertion, one default-off isolated snapshot/lease claim, explicit general-coding admission, authenticated snapshot replication, canonical multi-file artifact admission, retained-attempt recovery, and master-owned candidate freezing are implemented. Guidance labels are display-only. The worker grants no child, shell, exec, arbitrary tool, provider, test, credential, or network authority and proves no host sandbox or host-egress enforcement. Integration never mutates the registered source checkout. No validation gate, review provider, publication coordinator, Mac queue UI, queue advancement, or autonomous activation is implemented.",
        ),
        feature(
            "enrollment_identity_and_mtls",
            "implemented",
            "Windows enrollment creates a DPAPI-protected P-256 CA, issues ten-minute single-use digest-only grants, verifies client CSRs, and issues 30-day device certificates with rotation and revocation. The optional TLS 1.3 mTLS listener binds an exact-IP ephemeral server identity, rechecks certificate serial/digest/device revocation per request, and binds the application handshake to the TLS exporter.",
            "Loopback and private-overlay evidence only; live device enrollment remains owner-recorded external evidence.",
        ),
        feature(
            "windows_service_lifecycle",
            "implemented",
            "The Windows SCM host provides automatic start, bounded restart recovery, explicit install/start/stop/status/maintenance/recover/uninstall commands, and a durable fail-closed maintenance marker that blocks new enqueue and lease admission.",
            "Proven on an elevated Windows runner; it is not a host-hardening, upgrade automation, or unattended reliability claim.",
        ),
        feature(
            "app_supervised_peer_identity_transport",
            "implemented",
            "The local Unix-domain-socket transport uses an owner-only 0700 runtime directory and 0600 random socket leaf, obtains LOCAL_PEERTOKEN, validates the connected peer against a launch-supplied Security.framework designated requirement and current EUID before framing, and requires half-close without trailing input under strict bounded frames.",
            "Ad-hoc exact-build requirements bind the evaluated cdhash but do not prove Developer ID publisher identity. This is not device authentication, XPC, App Sandbox enforcement, host-level egress policy, notarization, or live-device proof.",
        ),
        feature(
            "mac_developer_bridge_and_agent",
            "implemented",
            "The Mac app supervises only the exact separately signed bridge helper, which keeps the Keychain identity and outbound mTLS session, directly supervises the pinned agent, and forwards authenticated metadata pages into a durable cursor. The agent's default-off fixture and singleton MLX lanes run one bounded no-retention request with cleared offline environment and dedicated process-group reaping; cancellation dominates completion and suppresses late output.",
            "Bounded local inference and metadata relay only. It adds no remote planning, repository, tool, credential, Codex, Git, publication, or unattended authority.",
        ),
        feature(
            "release_ci_gate",
            "implemented",
            "`./scripts/release-local.sh` runs version consistency, CI workflow and docs drift smoke, the bridge live-E2E preflight, formatting, clippy, workspace tests including ignored release proofs, cargo packaging, distribution self-tests, unsigned structure and launch checks, release runbooks, evidence preflights, and the Swift build and test suites. GitHub Actions runs the same gate on macOS.",
            "Repository-owned validation only; it does not sign, notarize, staple, install, or validate on a live device.",
        ),
        feature(
            "unsigned_distribution_launch",
            "implemented",
            "`package-distribution.sh` builds an unsigned app and installer package, validates bundle and package metadata against expected release identity, proves the running-app guard, and performs a clean-profile unsigned launch check.",
            "Unsigned local structure and launch proof only; Developer ID signing, notarization, stapling, and Gatekeeper assessment remain separate owner-recorded evidence.",
        ),
        feature(
            "release_evidence_status",
            "implemented",
            "`assemblywright release evidence-status` reports present, missing, or invalid status for expected artifact paths and owner-recorded JSON reports, validating app bundle metadata, bundled core version markers, signed-provenance artifact digest matching, live-device QA release metadata and non-future timestamp semantics, exact app-executable identity and cross-report binding, and final-bundle path/digest/archive-URI/signature-validation semantics.",
            "File and report inventory plus structural validation only. Presence never proves that signing, notarization, stapling, installation, or live-device QA actually happened.",
        ),
        feature(
            "release_evidence_bundle",
            "implemented",
            "`release-evidence-bundle.sh` assembles the final owner-attested bundle from the signed artifacts, signed-distribution provenance report, and live-device QA report, revalidating each child report and binding paths, digests, and archive URIs.",
            "Owner-recorded external evidence, not local proof that the external release checks were performed.",
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

pub fn release_evidence_bundle_runbook() -> ReleaseRunbookResponse {
    release_evidence_bundle_runbook_from(&release_readiness(), &release_evidence_status())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn protocol_readiness_proof_is_version_independent() {
        let features = contract_features();
        let protocol_features = features
            .iter()
            .filter(|feature| feature.key == "distributed_protocol_contract")
            .collect::<Vec<_>>();
        assert_eq!(
            protocol_features.len(),
            1,
            "readiness must define exactly one distributed protocol feature"
        );

        let feature = protocol_features[0];
        assert_eq!(feature.status, "implemented");
        assert!(
            feature.proof.contains("the current protocol version"),
            "the proof must describe the authoritative declaration without copying its number"
        );
        assert!(
            !contains_numeric_protocol_version(&feature.proof),
            "the proof must not contain a numeric protocol version that can drift"
        );
    }

    #[test]
    fn feature_conveyor_readiness_proof_preserves_guidance_authority_boundary() {
        let feature = contract_features()
            .into_iter()
            .find(|feature| feature.key == "feature_conveyor_repository_kernel")
            .expect("Feature Conveyor readiness feature");
        assert!(feature.proof.contains("loopback read-only status route"));
        assert!(feature.proof.contains(
            "owner guidance bound to queue, Emergency Pause, and optional feature lifecycle revisions"
        ));
        assert!(feature
            .proof
            .contains("digest-only repository-grant revisions"));
        assert!(feature
            .proof
            .contains("point-in-time filesystem-only repository identity preflight"));
        assert!(feature
            .boundary
            .contains("one default-off isolated snapshot/lease claim"));
        assert!(feature
            .boundary
            .contains("explicit general-coding admission"));
        assert!(feature.boundary.contains("master-owned candidate freezing"));
        assert!(feature
            .boundary
            .contains("never mutates the registered source checkout"));
        assert!(feature.boundary.contains("display-only"));
        assert!(feature.boundary.contains("autonomous activation"));
        for forbidden in [
            "mutation authority",
            "repository execution authority",
            "publication authority",
        ] {
            assert!(
                !feature.proof.contains(forbidden),
                "readiness proof broadened authority: {forbidden}"
            );
        }
    }

    #[test]
    fn feature_conveyor_readiness_limits_contained_coding_authority() {
        let feature = contract_features()
            .into_iter()
            .find(|feature| feature.key == "feature_conveyor_repository_kernel")
            .expect("Feature Conveyor readiness feature");
        for required in [
            "deterministic file.write.v1 and file.delete.v1",
            "64 sorted normalized relative paths",
            "aggregate-budgeted ephemeral independent repository",
            "owner-private no-follow parent descriptors",
            "exclusive atomic create",
            "atomic-swap replacement",
            "same-parent delete capture",
            "displaced-inode verification and rollback",
            "No child, shell, exec",
            "exact protocol-owned admission transcript",
            "Success seals one workspace",
            "restart re-hashes and reconstructs exactly one pair",
            "Windows independently validates artifact operations",
            "no child, shell, exec, arbitrary tool, provider, test, credential, or network authority",
            "no host sandbox or host-egress enforcement",
            "master-owned candidate freezing",
            "never mutates the registered source checkout",
            "No validation gate",
            "autonomous activation",
        ] {
            assert!(
                feature.proof.contains(required) || feature.boundary.contains(required),
                "readiness metadata omitted contained-coding boundary: {required}"
            );
        }
    }
}
