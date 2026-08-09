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
            #"{"schema_version":7,"queue_revision":1,"startup_quarantine_count":0,"counts_by_status":{"queued":1,"implementing":0,"validating":0,"reviewing":0,"publishing":0,"verifying_main":0,"succeeded":0,"cancelled":0,"abandoned":0,"quarantined":0},"visible_feature_count":1,"features_truncated":false,"features":[{"feature_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","specification_revision":1,"lifecycle_revision":1,"queue_position":1,"status":"queued","lease_present":false,"effect_possible":false}],"owner_guidance":{"state":"ready","reason_code":"head_dependency_satisfied","next_owner_action":"await_owner_control_surface","feature_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","specification_revision":1,"lifecycle_revision":1,"queue_revision":1,"emergency_pause_revision":0}}"#.utf8
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

private func featureConveyorStatus(
    state: AssemblywrightMacFeatureConveyorGuidanceState = .idle,
    nextOwnerAction: AssemblywrightMacFeatureConveyorNextOwnerAction = .prepareApprovedFeature
) -> AssemblywrightMacFeatureConveyorStatus {
    AssemblywrightMacFeatureConveyorStatus(
        schemaVersion: 7,
        queueRevision: 0,
        startupQuarantineCount: 0,
        countsByStatus: .init(
            queued: 0,
            implementing: 0,
            validating: 0,
            reviewing: 0,
            publishing: 0,
            verifyingMain: 0,
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
