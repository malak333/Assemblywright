import Foundation
import Testing
@testable import AssemblywrightMacApp
@testable import AssemblywrightMacCore

@MainActor
@Suite("Assemblywright Mac app presentation")
struct AssemblywrightMacAppTests {
    @Test("Developer Mode setup separates helper, pairing, and Windows readiness")
    func developerModeSetupPresentationIsFailClosed() {
        let missing = DeveloperModeSetupPresentation(
            configurationState: .notConfigured,
            bridgeStatus: .disabled,
            enrollmentInstalled: nil
        )
        #expect(missing.setupStatus == DeveloperModeSetupPresentation.setupRequired)
        #expect(missing.needsSetup)
        #expect(!missing.canRetry)

        let unpaired = DeveloperModeSetupPresentation(
            configurationState: .configured,
            bridgeStatus: .init(phase: .masterOffline, errorCode: "enrollment_not_installed"),
            enrollmentInstalled: false
        )
        #expect(unpaired.setupStatus == DeveloperModeSetupPresentation.helperReady)
        #expect(unpaired.pairingStatus == DeveloperModeSetupPresentation.pairingRequired)
        #expect(unpaired.needsPairing)
        #expect(unpaired.canRetry)

        let connected = DeveloperModeSetupPresentation(
            configurationState: .configured,
            bridgeStatus: .init(
                phase: .connected,
                deviceID: "11111111-1111-4111-8111-111111111111",
                masterEndpoint: "100.64.23.14:7792",
                connectionEpoch: 9
            ),
            enrollmentInstalled: true
        )
        #expect(connected.connectionStatus == DeveloperModeSetupPresentation.connected)
        #expect(!connected.needsSetup)
        #expect(!connected.needsPairing)
        #expect(!connected.canRetry)
    }

    @Test("Developer Mode setup mounts bounded pairing and recovery actions")
    func developerModeSetupWiring() throws {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let setupSource = try String(
            contentsOf: root.appendingPathComponent(
                "Sources/AssemblywrightMacApp/DeveloperModeSetupView.swift"
            ),
            encoding: .utf8
        )
        let ownerSource = try String(
            contentsOf: root.appendingPathComponent(
                "Sources/AssemblywrightMacApp/AssemblyLineOwnerView.swift"
            ),
            encoding: .utf8
        )

        for hook in [
            "developer-mode-save-helper",
            "developer-mode-check-pairing",
            "developer-mode-retry-connection",
            "developer-mode-prepare-enrollment",
            "developer-mode-install-enrollment"
        ] {
            #expect(setupSource.contains(hook))
        }
        #expect(setupSource.contains("enrollment rotate-pair"))
        #expect(setupSource.contains("developer-mode-rotation-acknowledgement"))
        #expect(setupSource.contains("rotate-recover-acknowledge"))
        #expect(setupSource.contains("rotationAcknowledgementGrantID"))
        #expect(!setupSource.contains("--grant-id <grant-id>"))
        #expect(setupSource.contains("Windows remains authoritative"))
        #expect(!setupSource.contains("grant_secret"))
        #expect(ownerSource.contains("DeveloperModeSetupView(model: developerBridge)"))
    }

    @Test("Simple owner flow defaults to Public and auto-run")
    func simpleOwnerFlowDefaults() {
        let presentation = AssemblyLineOwnerPresentation()

        #expect(presentation.projectVisibility == .public)
        #expect(presentation.autoRun)
        #expect(!presentation.autoRunControlEnabled)
        #expect(AssemblyLineProjectVisibility.allCases == [.public, .private])
    }

    @Test("Simple owner flow uses canonical GitHub URLs")
    func simpleOwnerFlowCanonicalizesGitHubURLs() throws {
        let canonical = try #require(
            CanonicalGitHubRepositoryURL("HTTPS://GitHub.com/Owner/Project.git")
        )
        #expect(canonical.value == "https://github.com/owner/project")
        #expect(
            CanonicalGitHubRepositoryURL("https://github.com/Owner/Project/")?.value
                == "https://github.com/owner/project"
        )
        #expect(CanonicalGitHubRepositoryURL("git@github.com:owner/project.git") == nil)
        #expect(CanonicalGitHubRepositoryURL("https://example.com/owner/project") == nil)
        #expect(CanonicalGitHubRepositoryURL("https://github.com/owner") == nil)
        #expect(CanonicalGitHubRepositoryURL("https://github.com/owner/project/issues") == nil)
        #expect(CanonicalGitHubRepositoryURL("https://github.com/owner/project?token=value") == nil)
    }

    @Test("Simple owner flow exposes only the three primary areas")
    func simpleOwnerFlowLabelsExcludeLegacyAuthoringFields() {
        let sectionLabels = [
            AssemblyLineOwnerPresentation.newProjectTitle,
            AssemblyLineOwnerPresentation.newFeatureTitle,
            AssemblyLineOwnerPresentation.assemblyLineTitle
        ]
        #expect(sectionLabels == ["New Project", "New Feature", "Assembly Line"])

        let ownerActions = [
            AssemblyLineOwnerPresentation.brainstormProjectLabel,
            AssemblyLineOwnerPresentation.brainstormFeatureLabel,
            AssemblyLineOwnerPresentation.startLabel,
            AssemblyLineOwnerPresentation.stopLabel,
            AssemblyLineOwnerPresentation.emergencyPauseLabel,
            AssemblyLineOwnerPresentation.recoveryLabel
        ]
            .joined(separator: " ")
            .lowercased()
        for legacyAction in ["activate feature conveyor", "cancel active", "abandon", "approve and enqueue"] {
            #expect(!ownerActions.contains(legacyAction))
        }
    }

    @Test("Pending Assembly Line mutation exposes exact-retry recovery and blocks conflicts")
    func simpleOwnerFlowPendingRecovery() throws {
        let presentation = AssemblyLineOwnerPresentation(
            pendingPlanningAction: .autoRun
        )

        #expect(presentation.recoveryRequired)
        #expect(presentation.recoveryStatus?.contains("exact saved request") == true)
        #expect(!presentation.autoRunControlEnabled)
        #expect(AssemblyLineOwnerPresentation.recoveryLabel == "Retry Exact Pending Action")

        let viewURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Sources/AssemblywrightMacApp/AssemblyLineOwnerView.swift")
        let source = try String(contentsOf: viewURL, encoding: .utf8)
        #expect(source.contains("reconcilePendingAssemblyLinePlanningMutation"))
        #expect(source.contains("assembly-line-reconcile-pending"))
    }

    @Test("Planning controls expose review confirmation and authoritative recovery wiring")
    func planningReviewAndCreationWiring() throws {
        let viewURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Sources/AssemblywrightMacApp/AssemblyLineOwnerView.swift")
        let source = try String(contentsOf: viewURL, encoding: .utf8)

        for hook in [
            "assembly-line-brainstorm-project",
            "assembly-line-brainstorm-feature",
            "assembly-line-approve-project",
            "assembly-line-approve-feature",
            "assembly-line-create-reconcile-repository"
        ] {
            #expect(source.contains(hook))
        }
        #expect(source.contains("confirmationDialog"))
        #expect(source.contains("projectBrainstormRequest"))
        #expect(source.contains("featureBrainstormRequest"))
        #expect(source.contains("ownerApprovalPreview"))
        #expect(source.contains("repositoryCreationRequest"))
        #expect(source.contains("Public information only"))
        #expect(source.contains("Send Public Idea to openai.codex"))
        #expect(source.contains("preview.requestData"))
        #expect(source.contains("invalidateProjectReview"))
        #expect(source.contains("invalidateFeatureReview"))
        #expect(source.contains("projectReviewGeneration == generation"))
        #expect(source.contains("featureReviewGeneration == generation"))
    }

    @Test("Frozen review binding denies approval after any effect input changes")
    func planningReviewBindingRejectsEdits() {
        let digest = Array(repeating: UInt8(0x44), count: 32)
        let binding = AssemblyLineFrozenReviewInputBinding(
            repositoryURL: "https://github.com/owner/project",
            visibility: .public,
            idea: "Build one public project",
            orchestratorCatalogSHA256: digest
        )

        #expect(binding.matches(
            repositoryURL: "https://github.com/owner/project",
            visibility: .public,
            idea: "Build one public project",
            orchestratorCatalogSHA256: digest
        ))
        #expect(!binding.matches(
            repositoryURL: "https://github.com/owner/edited",
            visibility: .public,
            idea: "Build one public project",
            orchestratorCatalogSHA256: digest
        ))
        #expect(!binding.matches(
            repositoryURL: "https://github.com/owner/project",
            visibility: .private,
            idea: "Build one public project",
            orchestratorCatalogSHA256: digest
        ))
        #expect(!binding.matches(
            repositoryURL: "https://github.com/owner/project",
            visibility: .public,
            idea: "Edited idea",
            orchestratorCatalogSHA256: digest
        ))
        #expect(!binding.matches(
            repositoryURL: "https://github.com/owner/project",
            visibility: .public,
            idea: "Build one public project",
            orchestratorCatalogSHA256: Array(repeating: UInt8(0x45), count: 32)
        ))
    }

    @Test("Owner confirmation names every exact frozen project effect binding")
    func planningApprovalConfirmationContents() {
        let preview = AssemblywrightMacOwnerApprovalPreview(
            requestData: Data(#"{"frozen":true}"#.utf8),
            targetKind: .project,
            repositoryURL: "https://github.com/owner/project",
            visibility: .private,
            specificationID: UUID(uuidString: "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA")!,
            specificationSHA256: Array(repeating: UInt8(0x11), count: 32),
            ownerApprovalSHA256: Array(repeating: UInt8(0x22), count: 32)
        )
        let confirmation = AssemblyLineOwnerApprovalConfirmationPresentation(preview)

        #expect(confirmation.repositoryURL == "https://github.com/owner/project")
        #expect(confirmation.visibility == "Private")
        #expect(confirmation.specificationID == "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
        #expect(confirmation.specificationSHA256 == String(repeating: "11", count: 32))
        #expect(confirmation.ownerApprovalSHA256 == String(repeating: "22", count: 32))
        #expect(confirmation.summary.contains("Repository: https://github.com/owner/project"))
        #expect(confirmation.summary.contains("Visibility: Private"))
        #expect(confirmation.summary.contains("Specification ID: aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"))
        #expect(confirmation.summary.contains("Specification SHA-256: \(String(repeating: "11", count: 32))"))
        #expect(confirmation.summary.contains("Owner approval SHA-256: \(String(repeating: "22", count: 32))"))
    }

    @Test("Developer details is a read-only projection of observed diagnostics")
    func simpleOwnerDeveloperDiagnosticsContent() {
        let diagnostics = AssemblyLineDeveloperDiagnosticsPresentation(
            status: AssemblywrightDeveloperBridgeAppStatus(
                phase: .connected,
                masterEndpoint: "100.64.23.14:7792",
                connectionEpoch: 45,
                featureConveyor: featureConveyorStatus()
            )
        )

        #expect(diagnostics.bridge == "Connected")
        #expect(diagnostics.master == "100.64.23.14:7792")
        #expect(diagnostics.connectionEpoch == "45")
        #expect(diagnostics.statusCode == nil)
        #expect(diagnostics.queue == "0 queued · 0 visible")

        let _: AssemblyLineDeveloperDiagnosticsView = .init(presentation: diagnostics)
    }

    @Test("Simple owner actions remain unavailable and Start also requires a feature")
    func simpleOwnerFlowFailsClosedUntilWindowsSupportExists() throws {
        var presentation = AssemblyLineOwnerPresentation()
        #expect(!presentation.hasQueuedFeature)
        #expect(!presentation.canStart)
        #expect(presentation.startReason == "Add at least one feature to start")
        #expect(!presentation.canStop)
        #expect(!presentation.canEmergencyPause)

        presentation.queuedFeatures = [
            AssemblyLineQueuedFeaturePresentation(
                id: UUID(),
                title: "First feature",
                repositoryURL: try #require(
                    CanonicalGitHubRepositoryURL("https://github.com/owner/project")
                )
            )
        ]
        #expect(presentation.hasQueuedFeature)
        #expect(!presentation.canStart)
        #expect(presentation.startReason == AssemblyLineOwnerPresentation.executionUnavailableReason)
    }

    @Test("Models presentation exposes one selectable lane and three fixed states")
    func modelsPresentationIsBounded() {
        let active = AssemblywrightMacLocalModelConfiguration(
            modelID: "Qwen3-8B",
            executablePath: "/private/owner/mlx_lm.generate",
            modelDirectoryPath: "/private/owner/models/qwen",
            registryRevision: 4
        )
        let presentation = LocalModelSelectionPresentation(
            state: .init(active: active, pending: nil)
        )
        #expect(presentation.macStatus == "Active: Qwen3-8B")
        #expect(presentation.localCoding.contains("fixed"))
        #expect(presentation.windowsRTX == "Not provisioned")
        #expect(presentation.productionReview.contains("gpt-5.6-sol"))

        let pending = AssemblywrightMacPendingLocalModelSelection(
            configuration: active,
            requestData: Data("{}".utf8)
        )
        #expect(LocalModelSelectionPresentation(
            state: .init(active: nil, pending: pending)
        ).macStatus == "Pending reconciliation: Qwen3-8B")
    }

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

    @Test("Production review binding is clearly separate from Codex development agents")
    func productionReviewBindingPresentationIsExplicit() {
        let form = ApprovedFeatureAuthoringForm()

        #expect(
            ApprovedFeatureReviewBindingPresentation.groupTitle
                == "Identity and production review binding"
        )
        #expect(
            ApprovedFeatureReviewBindingPresentation.providerLabel
                == "Production review provider"
        )
        #expect(
            ApprovedFeatureReviewBindingPresentation.modelLabel
                == "Production review model"
        )
        #expect(ApprovedFeatureReviewBindingPresentation.lockLabel == "Fixed by Windows master")
        #expect(
            ApprovedFeatureReviewBindingPresentation.accessibilityIdentifier
                == "approved-feature-production-review-binding"
        )
        #expect(
            ApprovedFeatureReviewBindingPresentation.explanation
                == "This Feature Conveyor binding is separate from repository-scoped Codex development reviewer agents."
        )
        #expect(form.providerID == "openai.codex")
        #expect(form.modelID == "gpt-5.6-sol")
    }

    @Test("Onboarding receipt import atomically prefills repository routing only")
    func onboardingReceiptImportPrefillsRepositoryRoutingOnly() throws {
        var form = validApprovedFeatureForm()
        form.repositoryID = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
        form.registrationGrantRevision = "8"
        form.cloudDisclosureGrantRevision = "9"
        form.autonomousPublicationGrantRevision = "10"
        form.baseBranch = "release-candidate"
        let originalOutcome = form.outcome
        let receipt = """
        {"schema_version":1,"status":"repository_onboarding_ready","repository_id":"cccccccc-cccc-4ccc-8ccc-cccccccccccc","registration_grant_revision":1,"cloud_disclosure_grant_revision":1,"autonomous_publication_grant_revision":1,"base_branch":"main","head_commit":"\(String(repeating: "a", count: 40))","scope_sha256":"\(String(repeating: "b", count: 64))","approval_plan_sha256":"\(String(repeating: "c", count: 64))","preflight_fingerprint_sha256":"\(String(repeating: "d", count: 64))"}
        """

        try form.importRepositoryOnboardingReceipt(receipt)

        #expect(form.repositoryID == "cccccccc-cccc-4ccc-8ccc-cccccccccccc")
        #expect(form.registrationGrantRevision == "1")
        #expect(form.cloudDisclosureGrantRevision == "1")
        #expect(form.autonomousPublicationGrantRevision == "1")
        #expect(form.baseBranch == "release-candidate")
        #expect(form.outcome == originalOutcome)

        let imported = form
        #expect(throws: AssemblywrightMacRepositoryOnboardingReceiptError.invalid) {
            try form.importRepositoryOnboardingReceipt("{}")
        }
        #expect(form == imported)
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
