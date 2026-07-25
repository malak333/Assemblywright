import Testing
import UserNotifications
@testable import JarvisMacApp
@testable import JarvisMacCore

@MainActor
@Suite("Assemblywright Mac app release presentation")
struct JarvisMacAppTests {
    @Test("Developer bridge presentation maps every read-only lifecycle state")
    func developerBridgePresentationMapsEveryLifecycleState() {
        let cases: [(JarvisDeveloperBridgeAppPhase, String)] = [
            (.disabled, "Disabled"),
            (.starting, "Starting"),
            (.connected, "Connected"),
            (.masterOffline, "Master Offline"),
            (.maintenance, "Maintenance"),
            (.paused, "Paused"),
            (.stopped, "Stopped")
        ]

        for (phase, expectedLabel) in cases {
            let presentation = DeveloperBridgeStatusPresentation(
                status: JarvisDeveloperBridgeAppStatus(phase: phase)
            )
            #expect(presentation.phaseLabel == expectedLabel)
        }
        #expect(JarvisDeveloperBridgeProcessLifecycle.proofBoundary.contains("Read-only"))
        #expect(JarvisDeveloperBridgeProcessLifecycle.proofBoundary.contains("does not enable"))
    }

    @Test("Plugin enablement confirmation states authority and containment boundaries")
    func pluginEnablementConfirmationIsExplicit() {
        let network = PluginEnablementConfirmation(
            pluginID: "release_uploader",
            pluginName: "Release Uploader",
            grant: "subprocess_stdio_network",
            lifecycleContractSha256: "network-lifecycle-digest",
            declaredHosts: ["api.example.com"]
        )
        let local = PluginEnablementConfirmation(
            pluginID: "local_worker",
            pluginName: "Local Worker",
            grant: "subprocess_stdio",
            lifecycleContractSha256: "local-lifecycle-digest",
            declaredHosts: []
        )
        let wasm = PluginEnablementConfirmation(
            pluginID: "local_compute",
            pluginName: "Local Compute",
            grant: "wasm_compute",
            lifecycleContractSha256: "wasm-lifecycle-digest",
            declaredHosts: []
        )

        #expect(network.message.contains("api.example.com"))
        #expect(network.message.contains("not OS sandboxed"))
        #expect(network.message.contains("host-level egress is not enforced"))
        #expect(network.message.contains("does not run the plugin"))
        #expect(local.message.contains("No network hosts are declared"))
        #expect(wasm.message.contains("WASM compute is confined"))
        #expect(wasm.message.contains("no imports, filesystem, network, environment, clock, or process authority"))
        #expect(!wasm.message.contains("Subprocess"))
        #expect(!wasm.message.contains("not OS sandboxed"))
        #expect(!wasm.message.contains("egress"))
        #expect(wasm.message.contains("does not run the plugin"))
        #expect(network.lifecycleContractSha256 == "network-lifecycle-digest")
        #expect(local.lifecycleContractSha256 == "local-lifecycle-digest")
        #expect(network.id.contains("network-lifecycle-digest"))
        #expect(local.id.contains("local-lifecycle-digest"))
    }

    @Test("Plugin update confirmation states version and disabled-authority boundary")
    func pluginUpdateConfirmationIsExplicitAndRedacted() {
        let confirmation = PluginUpdateConfirmation(
            pluginID: "local_worker",
            pluginName: "Local Worker",
            currentVersion: "1.0.0",
            candidateVersion: "1.1.0"
        )

        #expect(confirmation.message.contains("from 1.0.0 to 1.1.0"))
        #expect(confirmation.message.contains("same plugin ID"))
        #expect(confirmation.message.contains("newer version"))
        #expect(confirmation.message.contains("Execution will be disabled"))
        #expect(confirmation.message.contains("verify integrity"))
        #expect(!confirmation.message.contains("/Users/"))
        #expect(!confirmation.id.contains("sha256"))
    }

    @Test("Workspace grant presentation keeps the selected path hidden")
    func workspaceGrantPresentationIsRedacted() {
        let presentation = WorkspaceRootGrantPresentation(
            grant: JarvisWorkspaceRootGrant(id: "root_0123456789abcdef", status: "configured")
        )

        #expect(presentation.idLine == "root_0123456789abcdef")
        #expect(presentation.detailLine == "configured; authorized directory path hidden")
        #expect(!presentation.detailLine.contains("/Users/"))
    }

    @Test("Workspace activation presentation never claims an external core holds app grants")
    func workspaceActivationPresentationRequiresAppSupervision() {
        let supervised = WorkspaceRootActivationPresentation(isAvailable: true, isAppSupervised: true)
        let external = WorkspaceRootActivationPresentation(isAvailable: true, isAppSupervised: false)

        #expect(supervised.statusMessage.contains("are active"))
        #expect(external.statusMessage.contains("are not active"))
        #expect(external.statusMessage.contains("Another process"))
    }

    @Test("Menu bar contract preserves the stable main-window route")
    func menuBarContractPreservesMainWindowRoute() {
        #expect(JarvisMenuBarContract.mainWindowID == "jarvis-main")
        #expect(JarvisMenuBarContract.title == "Assemblywright")
    }

    @Test("Scheduler presentation surfaces durable notification acknowledgement failures")
    func schedulerPresentationSurfacesNotificationAcknowledgementFailures() {
        let acknowledgementError = SchedulerManagementPresentation.errorMessage(
            schedulerError: nil,
            notificationAcknowledgementError: "temporary IPC failure"
        )
        let schedulerError = SchedulerManagementPresentation.errorMessage(
            schedulerError: "scheduler unavailable",
            notificationAcknowledgementError: "temporary IPC failure"
        )

        #expect(acknowledgementError == "Notification acknowledgement pending retry: temporary IPC failure")
        #expect(schedulerError == "scheduler unavailable")
    }

    @Test("Menu bar presentation maps every supervisor lifecycle state")
    func menuBarPresentationMapsSupervisorLifecycle() {
        let stopped = JarvisMenuBarPresentation(mode: .stopped)
        let starting = JarvisMenuBarPresentation(mode: .starting)
        let available = JarvisMenuBarPresentation(mode: .available)
        let degraded = JarvisMenuBarPresentation(mode: .degraded(reason: "health check failed"))

        #expect(stopped.statusLine == "Core stopped")
        #expect(stopped.systemImage == "circle")
        #expect(stopped.canStartCore)
        #expect(!stopped.canStopCore)

        #expect(starting.statusLine == "Core starting")
        #expect(starting.systemImage == "circle.dotted")
        #expect(!starting.canStartCore)
        #expect(!starting.canStopCore)

        #expect(available.statusLine == "Core available")
        #expect(available.systemImage == "checkmark.circle.fill")
        #expect(!available.canStartCore)
        #expect(available.canStopCore)

        #expect(degraded.statusLine == "Core degraded")
        #expect(degraded.systemImage == "exclamationmark.triangle.fill")
        #expect(!degraded.canStartCore)
        #expect(degraded.canStopCore)
    }

    @Test("Console synchronization never probes IPC after a pre-authority failure")
    func consoleSynchronizationPreservesSupervisorFailure() {
        let failure = "Assemblywright app signature validation failed; quit and reopen Assemblywright."
        let available = CoreConsoleSynchronizationPresentation(mode: .available)
        let degraded = CoreConsoleSynchronizationPresentation(
            mode: .degraded(reason: failure)
        )
        let stopped = CoreConsoleSynchronizationPresentation(mode: .stopped)

        #expect(available.shouldRefreshHealth)
        #expect(available.unavailableReason == nil)
        #expect(!degraded.shouldRefreshHealth)
        #expect(degraded.unavailableReason == failure)
        #expect(!stopped.shouldRefreshHealth)
        #expect(stopped.unavailableReason == "Assemblywright core is stopped.")
    }

    @Test("Model tab presentation gates start and download around installed state")
    func modelTabPresentationGatesStartAndDownloadAroundInstalledState() {
        let missingModel = ModelConfigurationPresentation(
            canControlSelectedModelRuntime: true,
            canUpgradeLocalOllama: true,
            isWorking: false,
            selectedModelIsInstalled: false,
            downloadProgress: nil
        )
        let installedModel = ModelConfigurationPresentation(
            canControlSelectedModelRuntime: true,
            canUpgradeLocalOllama: true,
            isWorking: false,
            selectedModelIsInstalled: true,
            downloadProgress: nil
        )
        let busyModel = ModelConfigurationPresentation(
            canControlSelectedModelRuntime: true,
            canUpgradeLocalOllama: true,
            isWorking: true,
            selectedModelIsInstalled: true,
            downloadProgress: nil
        )

        #expect(!missingModel.canStartModel)
        #expect(missingModel.canDownloadSelected)
        #expect(missingModel.canStopModel)
        #expect(missingModel.canUpgradeOllama)
        #expect(installedModel.canStartModel)
        #expect(!installedModel.canDownloadSelected)
        #expect(installedModel.canStopModel)
        #expect(!busyModel.canStartModel)
        #expect(!busyModel.canDownloadSelected)
        #expect(!busyModel.canStopModel)
        #expect(!busyModel.canUpgradeOllama)
    }

    @Test("Model tab presentation exposes streamed download progress")
    func modelTabPresentationExposesStreamedDownloadProgress() {
        let presentation = ModelConfigurationPresentation(
            canControlSelectedModelRuntime: true,
            canUpgradeLocalOllama: true,
            isWorking: true,
            selectedModelIsInstalled: false,
            downloadProgress: JarvisOllamaPullProgress(
                status: "downloading",
                completedBytes: 25,
                totalBytes: 100
            )
        )

        #expect(presentation.progressValue == 0.25)
        #expect(presentation.progressDetailLine?.contains("downloading") == true)
        #expect(presentation.progressDetailLine?.contains("of") == true)
        #expect(!presentation.canStartModel)
        #expect(!presentation.canDownloadSelected)
    }

    @Test("Release readiness presentation blocks raw ready claims when effective evidence is incomplete")
    func releaseReadinessPresentationBlocksRawReadyClaimsWhenEvidenceIncomplete() {
        let readiness = JarvisReleaseReadiness(
            generatedAt: "2026-06-19T18:23:05Z",
            productionReady: true,
            evidenceModeEnabled: true,
            readinessScope: "external evidence mode",
            verifiedFeatureCount: 16,
            pendingFeatureCount: 0,
            implementedFeatures: [],
            pendingFeatures: [],
            blockingManualGates: [],
            recommendedVerificationCommands: [],
            proofBoundary: "Repo-owned readiness only; external evidence still gates production claims."
        )

        let presentation = ReleaseReadinessPresentation(
            readiness: readiness,
            effectiveProductionReady: false,
            isShowingStaleReadiness: false
        )

        #expect(presentation.productionReadyLine == "no")
        #expect(presentation.evidenceModeLine == "yes")
        #expect(presentation.staleWarning == nil)
        #expect(presentation.blockedReadinessWarning == "Readiness claim is blocked until current evidence status is complete.")
        #expect(!presentation.effectiveProductionReady)
    }

    @Test("Release readiness presentation exposes stale cached readiness warning")
    func releaseReadinessPresentationExposesStaleWarning() {
        let readiness = JarvisReleaseReadiness(
            generatedAt: "2026-06-19T18:23:05Z",
            productionReady: false,
            evidenceModeEnabled: false,
            readinessScope: "repo-owned checks",
            verifiedFeatureCount: 16,
            pendingFeatureCount: 1,
            implementedFeatures: [],
            pendingFeatures: [],
            blockingManualGates: ["live-device QA evidence recorded"],
            recommendedVerificationCommands: [],
            proofBoundary: "Cached readiness cannot clear production."
        )

        let presentation = ReleaseReadinessPresentation(
            readiness: readiness,
            effectiveProductionReady: false,
            isShowingStaleReadiness: true
        )

        #expect(presentation.productionReadyLine == "no")
        #expect(presentation.evidenceModeLine == "no")
        #expect(presentation.staleWarning == "Showing cached readiness; refresh failed.")
        #expect(presentation.blockedReadinessWarning == nil)
    }

    @Test("Release evidence row marks present presence-only items on the status line")
    func releaseEvidenceRowMarksPresenceOnlyItems() {
        let item = JarvisReleaseEvidenceStatusItem(
            key: "signed_app_zip",
            label: "Signed app zip path",
            path: "target/distribution/Assemblywright-0.1.4.zip",
            kind: "artifact",
            status: "present",
            requiredForProduction: true,
            manualGate: true,
            detail: "file exists; presence only; signing, notarization, and stapling are not validated by evidence-status"
        )

        let presentation = ReleaseEvidenceStatusPresentation(item: item)

        #expect(presentation.statusLine == "present; presence-only caveat")
        #expect(presentation.pathLine == "Path: target/distribution/Assemblywright-0.1.4.zip")
        #expect(presentation.detailLine == "Detail: file exists; presence only; signing, notarization, and stapling are not validated by evidence-status")
        #expect(presentation.requirementLine == "Required for production; manual evidence gate")
    }

    @Test("Release evidence row keeps non presence-only statuses unchanged")
    func releaseEvidenceRowKeepsOtherStatusesUnchanged() {
        let item = JarvisReleaseEvidenceStatusItem(
            key: "live_device_qa_report",
            label: "Live-device QA report",
            path: "target/release-live-device-qa-report.json",
            kind: "json_report",
            status: "missing",
            requiredForProduction: true,
            manualGate: true,
            detail: "expected JSON report is missing"
        )

        let presentation = ReleaseEvidenceStatusPresentation(item: item)

        #expect(presentation.statusLine == "missing")
    }

    @Test("Release evidence row exposes non-production informational context")
    func releaseEvidenceRowExposesInformationalContext() {
        let item = JarvisReleaseEvidenceStatusItem(
            key: "local_notes",
            label: "Local notes",
            path: "target/local-notes.json",
            kind: "json_report",
            status: "present",
            requiredForProduction: false,
            manualGate: false,
            detail: "optional local operator notes"
        )

        let presentation = ReleaseEvidenceStatusPresentation(item: item)

        #expect(presentation.statusLine == "present")
        #expect(presentation.pathLine == "Path: target/local-notes.json")
        #expect(presentation.detailLine == "Detail: optional local operator notes")
        #expect(presentation.requirementLine == "Informational evidence")
    }

    @Test("Release runbook presentation preserves every command and manual check")
    func releaseRunbookPresentationPreservesAllOperatorSteps() {
        let runbook = JarvisReleaseRunbook(
            generatedAt: "2026-06-12T12:00:00Z",
            generatedFrom: "release readiness plus evidence-status",
            runbook: "live_device",
            productionReady: false,
            liveVoiceFeature: nil,
            evidenceItems: [],
            commands: [
                "./scripts/release-live-device-qa.sh --check",
                "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env",
                "cargo run -p jarvis-cli -- command \"status check\" --endpoint <release-core-endpoint> --json",
                "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint <release-core-endpoint>"
            ],
            manualChecks: [
                "Install the signed package into /Applications on a clean Mac profile.",
                "Verify microphone and Speech permission prompts.",
                "Preserve target/release-live-device-qa-report.json for final release evidence bundling."
            ],
            proofBoundary: "Runbook only."
        )

        let presentation = ReleaseRunbookPresentation(
            runbook: runbook,
            effectiveProductionReady: false
        )

        #expect(presentation.readinessLine == "not ready")
        #expect(!presentation.isReady)
        #expect(presentation.commands == runbook.commands)
        #expect(presentation.manualChecks == runbook.manualChecks)
        #expect(presentation.commands.count == 4)
        #expect(presentation.manualChecks.count == 3)
    }

    @Test("Release runbook presentation blocks raw ready badge when effective readiness is false")
    func releaseRunbookPresentationBlocksRawReadyBadgeWhenEffectiveReadinessIsFalse() {
        let runbook = JarvisReleaseRunbook(
            generatedAt: "2026-06-19T18:23:05Z",
            generatedFrom: "release readiness plus evidence-status",
            runbook: "signed_distribution",
            productionReady: true,
            liveVoiceFeature: nil,
            evidenceItems: [],
            commands: [],
            manualChecks: [],
            proofBoundary: "Runbook only."
        )

        let blocked = ReleaseRunbookPresentation(
            runbook: runbook,
            effectiveProductionReady: false
        )
        let ready = ReleaseRunbookPresentation(
            runbook: runbook,
            effectiveProductionReady: true
        )

        #expect(blocked.readinessLine == "blocked")
        #expect(!blocked.isReady)
        #expect(ready.readinessLine == "ready")
        #expect(ready.isReady)
    }

    @Test("Release runbook presentation preserves live voice manual validation boundary")
    func releaseRunbookPresentationPreservesLiveVoiceManualValidationBoundary() {
        let liveVoiceFeature = JarvisReleaseReadinessFeature(
            key: "live_voice_loop",
            status: "pending_manual_validation",
            proof: "fake-adapter transcript staging and auto-submit tests",
            boundary: "manual validation pending"
        )
        let runbook = JarvisReleaseRunbook(
            generatedAt: "2026-06-19T18:23:05Z",
            generatedFrom: "release readiness plus evidence-status",
            runbook: "live_device",
            productionReady: false,
            liveVoiceFeature: liveVoiceFeature,
            evidenceItems: [],
            commands: [
                "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --json"
            ],
            manualChecks: [
                "Verify live microphone capture, Speech transcript, final transcript command submission, notification delivery, restart recovery, and audio output on a clean Mac profile."
            ],
            proofBoundary: "Runbook scaffolding only; no hardware validation is implied."
        )

        let presentation = ReleaseRunbookPresentation(
            runbook: runbook,
            effectiveProductionReady: false
        )

        #expect(presentation.readinessLine == "not ready")
        #expect(!presentation.isReady)
        #expect(presentation.liveVoiceFeatureLine == "live_voice_loop: pending_manual_validation")
        #expect(presentation.commands.first?.contains("JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external") == true)
        #expect(presentation.manualChecks.first?.contains("clean Mac profile") == true)
    }

    @Test("Mac scheduler notification adapter requests alert and sound authorization")
    func macSchedulerNotificationAdapterRequestsExpectedAuthorizationOptions() async throws {
        let notificationCenter = CapturingUserNotificationCenter(authorizationResult: true)
        let adapter = MacSchedulerNotificationAdapter(notificationCenter: notificationCenter)

        let authorized = try await adapter.requestAuthorization()

        #expect(authorized)
        #expect(notificationCenter.requestedAuthorizationOptions?.contains(.alert) == true)
        #expect(notificationCenter.requestedAuthorizationOptions?.contains(.sound) == true)
    }

    @Test("Mac scheduler notification adapter does not require app bundle for SwiftPM launch")
    func macSchedulerNotificationAdapterFallsBackOutsideAppBundle() async throws {
        let notificationCenter = MacSchedulerNotificationAdapter.defaultNotificationCenter(
            bundleURL: URL(fileURLWithPath: "/tmp/JarvisSwiftPM/.build/arm64-apple-macosx/debug")
        )
        let adapter = MacSchedulerNotificationAdapter(notificationCenter: notificationCenter)

        let authorized = try await adapter.requestAuthorization()

        #expect(!authorized)
    }

    @Test("Mac scheduler notification adapter preserves scheduler payload")
    func macSchedulerNotificationAdapterPreservesSchedulerPayload() async throws {
        let notificationCenter = CapturingUserNotificationCenter(authorizationResult: true)
        let adapter = MacSchedulerNotificationAdapter(notificationCenter: notificationCenter)
        let schedulerJobId = UUID(uuidString: "00000000-0000-4000-8000-000000000123")!
        let occurrenceID = UUID(uuidString: "00000000-0000-4000-8000-000000000456")!
        let request = JarvisSchedulerNotificationRequest(
            id: "scheduler-\(schedulerJobId.uuidString)-due_now",
            schedulerJobId: schedulerJobId,
            title: "Assemblywright scheduler job due",
            body: "A scheduler job is due and ready for the app to surface.",
            notificationKind: "due_now",
            threadIdentifier: "jarvis.scheduler",
            schedulerNotificationOccurrenceId: occurrenceID,
            schedulerNotificationRevision: 3
        )

        try await adapter.deliver(request)

        let deliveredRequest = try #require(notificationCenter.deliveredRequests.first)
        #expect(deliveredRequest.identifier == request.id)
        #expect(deliveredRequest.trigger == nil)
        #expect(deliveredRequest.content.title == request.title)
        #expect(deliveredRequest.content.body == request.body)
        #expect(deliveredRequest.content.threadIdentifier == request.threadIdentifier)
        #expect(deliveredRequest.content.sound != nil)
        #expect(deliveredRequest.content.userInfo["scheduler_job_id"] as? String == schedulerJobId.uuidString)
        #expect(deliveredRequest.content.userInfo["notification_kind"] as? String == request.notificationKind)
        #expect(
            deliveredRequest.content.userInfo["scheduler_notification_occurrence_id"] as? String
                == occurrenceID.uuidString
        )
        #expect(deliveredRequest.content.userInfo["scheduler_notification_revision"] as? UInt64 == 3)
    }

    @Test("Scheduler notification evidence presentation exposes release QA fields")
    func schedulerNotificationEvidencePresentationExposesReleaseQAFields() {
        let schedulerJobId = UUID(uuidString: "00000000-0000-4000-8000-000000000124")!
        let request = JarvisSchedulerNotificationRequest(
            id: "scheduler-\(schedulerJobId.uuidString)-failed",
            schedulerJobId: schedulerJobId,
            title: "Scheduler job failed: nightly sync",
            body: "Nightly sync failed after retry exhaustion.",
            notificationKind: "failed",
            threadIdentifier: "jarvis.scheduler"
        )

        let presentation = SchedulerNotificationEvidencePresentation(request: request)

        #expect(presentation.summary.contains("JARVIS_QA_NOTIFICATION_KIND=failed"))
        #expect(presentation.summary.contains("JARVIS_QA_NOTIFICATION_TITLE=Scheduler job failed: nightly sync"))
        #expect(presentation.summary.contains("JARVIS_QA_NOTIFICATION_BODY=Nightly sync failed after retry exhaustion."))
        #expect(presentation.summary.contains("JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER=jarvis.scheduler"))
    }

    @Test("Speech output evidence presentation exposes live-device QA fields before playback")
    func speechOutputEvidencePresentationExposesBlankCaptureFields() {
        let presentation = SpeechOutputEvidencePresentation(
            statusText: "Speech output idle.",
            lastSpokenText: nil
        )

        #expect(presentation.deviceLabelField == "JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL=<record actual output device>")
        #expect(presentation.evidenceNoteField.contains("JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="))
        #expect(presentation.evidenceNoteField.contains("Record playback observation after Speak Preview"))
        #expect(presentation.evidenceNoteField.contains("Speech output idle."))
    }

    @Test("Speech output evidence presentation includes last spoken preview text")
    func speechOutputEvidencePresentationIncludesLastSpokenText() {
        let presentation = SpeechOutputEvidencePresentation(
            statusText: "Speech output speaking.",
            lastSpokenText: "  Assemblywright status ready.  "
        )

        #expect(presentation.evidenceNoteField.contains("Observed playback for \"Assemblywright status ready.\""))
        #expect(presentation.evidenceNoteField.contains("Speech output speaking."))
    }
}

private final class CapturingUserNotificationCenter: JarvisUserNotificationCenter, @unchecked Sendable {
    let authorizationResult: Bool
    var requestedAuthorizationOptions: UNAuthorizationOptions?
    var deliveredRequests: [UNNotificationRequest]

    init(authorizationResult: Bool) {
        self.authorizationResult = authorizationResult
        self.deliveredRequests = []
    }

    func authorizationStatus() async -> UNAuthorizationStatus {
        authorizationResult ? .authorized : .denied
    }

    func requestAuthorization(options: UNAuthorizationOptions) async throws -> Bool {
        requestedAuthorizationOptions = options
        return authorizationResult
    }

    func add(_ request: UNNotificationRequest) async throws {
        deliveredRequests.append(request)
    }
}
