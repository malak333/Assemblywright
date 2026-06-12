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
}
