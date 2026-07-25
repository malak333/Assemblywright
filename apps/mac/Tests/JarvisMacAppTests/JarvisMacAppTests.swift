import Testing
@testable import JarvisMacApp
@testable import JarvisMacCore

@MainActor
@Suite("Assemblywright Mac app presentation")
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

    @Test("Menu bar contract preserves the stable main-window route")
    func menuBarContractPreservesMainWindowRoute() {
        #expect(JarvisMenuBarContract.mainWindowID == "assemblywright-main")
        #expect(JarvisMenuBarContract.title == "Assemblywright")
    }

    @Test("Menu bar presentation maps every bridge lifecycle state")
    func menuBarPresentationMapsBridgeLifecycle() {
        let cases: [(JarvisDeveloperBridgeAppPhase, String, String)] = [
            (.disabled, "Developer Mode disabled", "circle"),
            (.starting, "Bridge starting", "circle.dotted"),
            (.connected, "Bridge connected", "checkmark.circle.fill"),
            (.masterOffline, "Master offline", "exclamationmark.triangle.fill"),
            (.maintenance, "Master maintenance", "wrench.and.screwdriver.fill"),
            (.paused, "Bridge paused", "pause.circle.fill"),
            (.stopped, "Bridge stopped", "circle")
        ]

        for (phase, expectedStatusLine, expectedImage) in cases {
            let presentation = JarvisMenuBarPresentation(
                status: JarvisDeveloperBridgeAppStatus(phase: phase)
            )
            #expect(presentation.statusLine == expectedStatusLine)
            #expect(presentation.systemImage == expectedImage)
        }
    }

    @Test("Menu bar badges every state that needs attention and only those")
    func menuBarBadgesOnlyStatesNeedingAttention() {
        for phase in [
            JarvisDeveloperBridgeAppPhase.disabled,
            .starting,
            .masterOffline,
            .maintenance,
            .paused,
            .stopped
        ] {
            #expect(
                JarvisMenuBarPresentation(
                    status: JarvisDeveloperBridgeAppStatus(phase: phase)
                ).showsStateBadge
            )
        }

        // A connected bridge shows the proofmark alone, with no badge competing with it.
        #expect(
            !JarvisMenuBarPresentation(
                status: JarvisDeveloperBridgeAppStatus(phase: .connected)
            ).showsStateBadge
        )
    }

    @Test("Menu bar template art loads as template art when the bundle carries it")
    func menuBarTemplateArtIsTemplateWhenBundled() {
        #expect(JarvisBrandAssets.menuBarTemplateName == "menubar-template")

        // Unbundled test runs have no Resources directory, so the lookup is
        // expected to miss. When it does resolve, it must be template art or
        // AppKit would paint raw black pixels into the menu bar.
        if let template = JarvisBrandAssets.menuBarTemplate() {
            #expect(template.isTemplate)
        }
    }
}
