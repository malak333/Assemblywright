import Testing
@testable import JarvisMacApp
@testable import JarvisMacCore

@MainActor
@Suite("Jarvis Mac app release presentation")
struct JarvisMacAppTests {
    @Test("Release evidence row marks present presence-only items on the status line")
    func releaseEvidenceRowMarksPresenceOnlyItems() {
        let item = JarvisReleaseEvidenceStatusItem(
            key: "signed_app_zip",
            label: "Signed app zip path",
            path: "target/distribution/Jarvis-0.1.4.zip",
            kind: "artifact",
            status: "present",
            requiredForProduction: true,
            manualGate: true,
            detail: "file exists; presence only; signing, notarization, and stapling are not validated by evidence-status"
        )

        let presentation = ReleaseEvidenceStatusPresentation(item: item)

        #expect(presentation.statusLine == "present; presence-only caveat")
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

        let presentation = ReleaseRunbookPresentation(runbook: runbook)

        #expect(presentation.commands == runbook.commands)
        #expect(presentation.manualChecks == runbook.manualChecks)
        #expect(presentation.commands.count == 4)
        #expect(presentation.manualChecks.count == 3)
    }
}
