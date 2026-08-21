import Foundation
import Testing
@testable import AssemblywrightMacApp
@testable import AssemblywrightMacCore

@MainActor
@Suite("Assemblywright Mac app presentation")
struct AssemblywrightMacAppTests {
    @Test("Developer bridge presentation maps every read-only lifecycle state")
    func developerBridgePresentationMapsEveryLifecycleState() {
        let cases: [(AssemblywrightDeveloperBridgeAppPhase, String)] = [
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
                status: AssemblywrightDeveloperBridgeAppStatus(phase: phase)
            )
            #expect(presentation.phaseLabel == expectedLabel)
        }
        #expect(AssemblywrightDeveloperBridgeProcessLifecycle.proofBoundary.contains("Read-only"))
        #expect(AssemblywrightDeveloperBridgeProcessLifecycle.proofBoundary.contains("does not enable"))
    }

    @Test("Feature Conveyor presentation is compact read-only guidance")
    func featureConveyorPresentationIsReadOnlyGuidance() throws {
        let data = Data(
            #"{"schema_version":9,"queue_revision":1,"startup_quarantine_count":0,"counts_by_status":{"queued":1,"implementing":0,"validating":0,"reviewing":0,"publishing":0,"verifying_main":0,"repairing":0,"paused":0,"attention_required":0,"failed":0,"succeeded":0,"cancelled":0,"abandoned":0,"quarantined":0},"visible_feature_count":1,"features_truncated":false,"features":[{"feature_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","specification_revision":1,"lifecycle_revision":1,"queue_position":1,"status":"queued","lease_present":false,"effect_possible":false}],"owner_guidance":{"state":"ready","reason_code":"head_dependency_satisfied","next_owner_action":"await_owner_control_surface","feature_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","specification_revision":1,"lifecycle_revision":1,"queue_revision":1,"emergency_pause_revision":0}}"#.utf8
        )
        let status = try JSONDecoder().decode(
            AssemblywrightMacFeatureConveyorStatus.self,
            from: data
        )

        let presentation = FeatureConveyorStatusPresentation(status: status)

        #expect(presentation.queueLabel == "1 queued · 1 visible")
        #expect(presentation.stateLabel == "Ready")
        #expect(presentation.guidanceLabel == "Await owner control surface")
        #expect(presentation.currentFeatureLabel == "aaaaaaaa · queued")
    }

    @Test("Feature Conveyor presentation maps every fixed state and owner-action label")
    func featureConveyorPresentationMapsEveryFixedLabel() {
        let stateCases: [(AssemblywrightMacFeatureConveyorGuidanceState, String)] = [
            (.idle, "Idle"),
            (.ready, "Ready"),
            (.blocked, "Blocked"),
            (.inProgress, "In progress")
        ]
        let actionCases: [(AssemblywrightMacFeatureConveyorNextOwnerAction, String)] = [
            (.prepareApprovedFeature, "Prepare an approved feature"),
            (.awaitOwnerControlSurface, "Await owner control surface"),
            (.resolveHeadDependency, "Resolve the head dependency"),
            (.wait, "Wait"),
            (.reconcileActiveFeature, "Reconcile the active feature"),
            (.resumeEmergencyPause, "Resume Emergency Pause deliberately")
        ]

        for (state, expectedLabel) in stateCases {
            let presentation = FeatureConveyorStatusPresentation(
                status: featureConveyorStatus(state: state)
            )
            #expect(presentation.stateLabel == expectedLabel)
            #expect(presentation.currentFeatureLabel == nil)
        }

        for (action, expectedLabel) in actionCases {
            let presentation = FeatureConveyorStatusPresentation(
                status: featureConveyorStatus(nextOwnerAction: action)
            )
            #expect(presentation.guidanceLabel == expectedLabel)
            #expect(presentation.currentFeatureLabel == nil)
        }
    }

    @Test("Menu bar contract preserves the stable main-window route")
    func menuBarContractPreservesMainWindowRoute() {
        #expect(AssemblywrightMenuBarContract.mainWindowID == "assemblywright-main")
        #expect(AssemblywrightMenuBarContract.title == "Assemblywright")
    }

    @Test("Owner activation preview shows blockers and never enables an unsigned app action")
    func ownerActivationPreviewGatesAppAction() throws {
        let ready = FeatureConveyorOwnerControlPresentation(
            control: try ownerControlProjection(completeEvidence: true)
        )
        #expect(ready.activationLabel == "Ready for owner confirmation")
        #expect(ready.blockerLabel == "None")
        #expect(ready.evidenceLabel == "6 of 6 Windows-admitted categories ready")

        let blocked = FeatureConveyorOwnerControlPresentation(
            control: try ownerControlProjection(emergencyPaused: true)
        )
        #expect(blocked.blockerLabel == "Emergency Pause")
        #expect(blocked.evidenceDigests.allSatisfy { $0.contains("missing") })
    }

    @Test("Approved-feature form creates a typed draft without handwritten JSON")
    func approvedFeatureFormCreatesTypedDraft() throws {
        let featureID = UUID(uuidString: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!
        let repositoryID = UUID(uuidString: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")!
        var form = ApprovedFeatureAuthoringForm()
        form.featureID = featureID.uuidString.uppercased()
        form.repositoryID = repositoryID.uuidString.lowercased()
        form.title = "Bounded authoring"
        form.outcome = "Submit one owner-approved feature"
        form.scope = "Mac owner interface only"
        form.acceptance = "typed-request\nexplicit-confirmation"
        form.allowedPaths = "apps/mac/Sources\napps/mac/Tests"
        form.designSHA256 = String(repeating: "A1", count: 32)
        form.brainstormingSHA256 = String(repeating: "b2", count: 32)
        form.ownerApprovalSHA256 = String(repeating: "c3", count: 32)
        form.assumptions = "Windows remains authoritative"
        form.prohibitedData = "credentials\nraw brainstorming transcript"

        let draft = try #require(form.draft())

        #expect(draft.featureID == featureID)
        #expect(draft.repositoryID == repositoryID)
        #expect(draft.manifest.acceptance == ["typed-request", "explicit-confirmation"])
        #expect(draft.manifest.allowedPaths == ["apps/mac/Sources", "apps/mac/Tests"])
        #expect(draft.manifest.publicationChecks == ["release-local", "protocol-windows"])
        #expect(draft.providerID == "openai.codex")
        #expect(draft.modelID == "gpt-5.6-sol")
        #expect(draft.designSHA256 == Array(repeating: 0xa1, count: 32))
        #expect(try draft.canonicalManifestData().count > 0)
    }

    @Test("Production Swift authoring bytes match the Rust strict-decode fixture")
    func approvedFeatureAuthoringMatchesRustFixture() throws {
        let draft = try #require(validApprovedFeatureForm().draft())
        let status = AssemblywrightDeveloperBridgeAppStatus(
            phase: .connected,
            featureConveyor: featureConveyorStatus(),
            ownerControl: try ownerControlProjection()
        )
        let encoded = try draft.encodeRequest(from: status)
        let fixtureURL = repositoryRootURL()
            .appendingPathComponent(
                "crates/assemblywright-protocol/tests/fixtures/approved_feature_authoring_request.json"
            )
        var fixture = try Data(contentsOf: fixtureURL)
        if fixture.last == 0x0a { fixture.removeLast() }

        #expect(encoded == fixture)
    }

    @Test("Approved-feature form rejects incomplete, duplicate, self-dependent, and secret-shaped input")
    func approvedFeatureFormRejectsUnsafeInput() {
        var form = validApprovedFeatureForm()
        form.repositoryID = ""
        #expect(form.draft() == nil)

        form = validApprovedFeatureForm()
        form.acceptance = "same\nsame"
        #expect(form.draft() == nil)

        form = validApprovedFeatureForm()
        form.dependencies = form.featureID
        #expect(form.draft() == nil)

        form = validApprovedFeatureForm()
        form.outcome = "Bearer this-is-secret-shaped"
        #expect(form.draft() == nil)

        form = validApprovedFeatureForm()
        form.outcome = "Never include embedded ghp_12345678901234567890 here"
        #expect(form.draft() == nil)

        form = validApprovedFeatureForm()
        form.outcome = "The redacted field contained bearer embedded-value"
        #expect(form.draft() == nil)

        form = validApprovedFeatureForm()
        form.designSHA256 = String(repeating: "0", count: 64)
        #expect(form.draft() == nil)
    }

    @Test("Approved-feature confirmation summarizes the exact bounded frozen draft")
    func approvedFeatureConfirmationSummaryIsExactAndBounded() throws {
        var form = validApprovedFeatureForm()
        form.dependencies = "cccccccc-cccc-4ccc-8ccc-cccccccccccc"
        let draft = try #require(form.draft())
        let prepared = try draft.prepareRequest(from: approvedFeatureReviewStatus())
        let summary = try #require(
            ApprovedFeatureConfirmationSummary(preparedRequest: prepared)
        )

        #expect(summary.featureID == "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
        #expect(summary.repositoryID == "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")
        #expect(summary.title == "Bounded authoring")
        #expect(summary.outcome == "Submit one owner-approved feature")
        #expect(summary.manifestSHA256.count == 64)
        #expect(summary.grants.contains("registration r1"))
        #expect(summary.providerModel == "openai.codex / gpt-5.6-sol")
        #expect(summary.dependencyIDs == ["cccccccc-cccc-4ccc-8ccc-cccccccccccc"])
        #expect(summary.designDigestPrefix == "111111111111")
        #expect(summary.brainstormingDigestPrefix == "222222222222")
        #expect(summary.ownerApprovalDigestPrefix == "333333333333")
        #expect(summary.queueRevision == 0)
        #expect(summary.ownerControlDesignationRevision == 1)
        #expect(summary.deviceID == "22222222-2222-4222-8222-222222222222")
        #expect(summary.connectionEpoch == 44)
        #expect(!summary.emergencyPaused)
        #expect(summary.emergencyPauseRevision == 0)
        #expect(summary.exactRequestSHA256.count == 64)
        #expect(summary.message.contains("Queue revision: 0"))
        #expect(summary.message.contains("connection epoch 44"))
        #expect(summary.message.utf8.count <= ApprovedFeatureConfirmationSummary.maximumMessageBytes)
    }

    @Test("Approved-feature review cannot open from incomplete paused or stale status")
    func approvedFeatureReviewRequiresExactAuthenticatedSnapshot() throws {
        let form = validApprovedFeatureForm()
        let valid = try approvedFeatureReviewStatus()
        #expect(form.preparedRequest(from: valid) != nil)
        #expect(form.preparedRequest(from: .init(phase: .masterOffline)) == nil)

        let missingDevice = AssemblywrightDeveloperBridgeAppStatus(
            phase: .connected,
            connectionEpoch: 44,
            featureConveyor: featureConveyorStatus(),
            ownerControl: try ownerControlProjection()
        )
        #expect(form.preparedRequest(from: missingDevice) == nil)
        let paused = try approvedFeatureReviewStatus(emergencyPaused: true)
        #expect(form.preparedRequest(from: paused) == nil)

        let stale = AssemblywrightDeveloperBridgeAppStatus(
            phase: .connected,
            deviceID: "22222222-2222-4222-8222-222222222222",
            connectionEpoch: 44,
            featureConveyor: featureConveyorStatus(),
            ownerControl: try ownerControlProjection(queueRevision: 1)
        )
        #expect(form.preparedRequest(from: stale) == nil)
    }

    @Test("Successful enqueue reset preserves repository routing but clears feature approval content")
    func approvedFeatureFormResetPreservesRepositoryRoutingOnly() {
        var form = validApprovedFeatureForm()
        let previousFeatureID = form.featureID
        let repositoryID = form.repositoryID

        form.resetAfterSuccessfulEnqueue()

        #expect(form.featureID != previousFeatureID)
        #expect(form.repositoryID == repositoryID)
        #expect(form.providerID == "openai.codex")
        #expect(form.registrationGrantRevision == "1")
        #expect(form.outcome.isEmpty)
        #expect(form.acceptance.isEmpty)
        #expect(form.designSHA256.isEmpty)
        #expect(form.draft() == nil)
    }

    @Test("Menu bar presentation maps every bridge lifecycle state")
    func menuBarPresentationMapsBridgeLifecycle() {
        let cases: [(AssemblywrightDeveloperBridgeAppPhase, String, String)] = [
            (.disabled, "Developer Mode disabled", "circle"),
            (.starting, "Bridge starting", "circle.dotted"),
            (.connected, "Bridge connected", "checkmark.circle.fill"),
            (.masterOffline, "Master offline", "exclamationmark.triangle.fill"),
            (.maintenance, "Master maintenance", "wrench.and.screwdriver.fill"),
            (.paused, "Bridge paused", "pause.circle.fill"),
            (.stopped, "Bridge stopped", "circle")
        ]

        for (phase, expectedStatusLine, expectedImage) in cases {
            let presentation = AssemblywrightMenuBarPresentation(
                status: AssemblywrightDeveloperBridgeAppStatus(phase: phase)
            )
            #expect(presentation.statusLine == expectedStatusLine)
            #expect(presentation.systemImage == expectedImage)
        }
    }

    @Test("Menu bar badges every state that needs attention and only those")
    func menuBarBadgesOnlyStatesNeedingAttention() {
        for phase in [
            AssemblywrightDeveloperBridgeAppPhase.disabled,
            .starting,
            .masterOffline,
            .maintenance,
            .paused,
            .stopped
        ] {
            #expect(
                AssemblywrightMenuBarPresentation(
                    status: AssemblywrightDeveloperBridgeAppStatus(phase: phase)
                ).showsStateBadge
            )
        }

        // A connected bridge shows the proofmark alone, with no badge competing with it.
        #expect(
            !AssemblywrightMenuBarPresentation(
                status: AssemblywrightDeveloperBridgeAppStatus(phase: .connected)
            ).showsStateBadge
        )
    }

    @Test("Menu bar template art loads as template art when the bundle carries it")
    func menuBarTemplateArtIsTemplateWhenBundled() {
        #expect(AssemblywrightBrandAssets.menuBarTemplateName == "menubar-template")

        // Unbundled test runs have no Resources directory, so the lookup is
        // expected to miss. When it does resolve, it must be template art or
        // AppKit would paint raw black pixels into the menu bar.
        if let template = AssemblywrightBrandAssets.menuBarTemplate() {
            #expect(template.isTemplate)
        }
    }
}

private func validApprovedFeatureForm() -> ApprovedFeatureAuthoringForm {
    var form = ApprovedFeatureAuthoringForm()
    form.featureID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
    form.repositoryID = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
    form.title = "Bounded authoring"
    form.outcome = "Submit one owner-approved feature"
    form.acceptance = "typed-request"
    form.designSHA256 = String(repeating: "11", count: 32)
    form.brainstormingSHA256 = String(repeating: "22", count: 32)
    form.ownerApprovalSHA256 = String(repeating: "33", count: 32)
    return form
}

private func approvedFeatureReviewStatus(
    emergencyPaused: Bool = false
) throws -> AssemblywrightDeveloperBridgeAppStatus {
    AssemblywrightDeveloperBridgeAppStatus(
        phase: .connected,
        deviceID: "22222222-2222-4222-8222-222222222222",
        masterEndpoint: "100.64.23.14:7792",
        connectionEpoch: 44,
        featureConveyor: featureConveyorStatus(),
        ownerControl: try ownerControlProjection(emergencyPaused: emergencyPaused)
    )
}

private func repositoryRootURL() -> URL {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0 ..< 5 { url.deleteLastPathComponent() }
    return url
}

private func ownerControlProjection(
    emergencyPaused: Bool = false,
    completeEvidence: Bool = false,
    queueRevision: UInt64 = 0
) throws -> AssemblywrightMacFeatureConveyorOwnerControlProjection {
    let names = ["repository_gate_proof", "restricted_worker_live", "review_provider_live",
                 "github_publication_live", "restart_recovery_live", "mac_windows_control_event_streaming_live"]
    var evidence: [String: Any] = [:]
    for (index, name) in names.enumerated() {
        evidence[name] = completeEvidence ? [
            "evidence_id": String(format: "%08x-0000-4000-8000-%012x", index + 1, index + 1),
            "revision": 1,
            "receipt_sha256": Array(repeating: index + 1, count: 32)
        ] : NSNull()
    }
    let ready = !emergencyPaused && completeEvidence
    let object: [String: Any] = [
        "schema_version": 1, "queue_revision": queueRevision,
        "emergency_paused": emergencyPaused, "emergency_pause_revision": emergencyPaused ? 1 : 0,
        "owner_control_designation_revision": 1, "activation_status": "inactive",
        "activation_id": NSNull(), "activation_ready": ready,
        "activation_blocker": emergencyPaused ? "emergency_paused" : completeEvidence ? "none" : "evidence_required",
        "active_feature": NSNull(), "evidence": evidence
    ]
    return try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
        JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    )
}

private func featureConveyorStatus(
    state: AssemblywrightMacFeatureConveyorGuidanceState = .idle,
    nextOwnerAction: AssemblywrightMacFeatureConveyorNextOwnerAction = .prepareApprovedFeature
) -> AssemblywrightMacFeatureConveyorStatus {
    AssemblywrightMacFeatureConveyorStatus(
        schemaVersion: 9,
        queueRevision: 0,
        startupQuarantineCount: 0,
        countsByStatus: .init(
            queued: 0,
            implementing: 0,
            validating: 0,
            reviewing: 0,
            publishing: 0,
            verifyingMain: 0,
            repairing: 0,
            paused: 0,
            attentionRequired: 0,
            failed: 0,
            succeeded: 0,
            cancelled: 0,
            abandoned: 0,
            quarantined: 0
        ),
        visibleFeatureCount: 0,
        featuresTruncated: false,
        features: [],
        ownerGuidance: .init(
            state: state,
            reasonCode: .queueEmpty,
            nextOwnerAction: nextOwnerAction,
            featureID: nil,
            specificationRevision: nil,
            lifecycleRevision: nil,
            queueRevision: 0,
            emergencyPauseRevision: 0
        )
    )
}
