import AssemblywrightMacCore
import Foundation
import SwiftUI

struct ApprovedFeatureAuthoringSection: View {
    @ObservedObject var model: AssemblywrightDeveloperBridgeProcessLifecycle
    @State private var form = ApprovedFeatureAuthoringForm()
    @State private var confirmationRequest: AssemblywrightMacApprovedFeaturePreparedRequest?
    @State private var reconciliationConfirmationPresented = false
    @State private var onboardingReceiptText = ""
    @State private var onboardingReceiptImportStatus: OnboardingReceiptImportStatus?

    private var draft: AssemblywrightMacFeatureConveyorApprovedFeatureDraft? {
        form.draft()
    }

    private var preparedRequest: AssemblywrightMacApprovedFeaturePreparedRequest? {
        form.preparedRequest(from: model.status)
    }

    private var authoringAvailable: Bool {
        model.status.phase == .connected
            && model.status.featureConveyor != nil
            && model.status.ownerControl?.emergencyPaused == false
            && model.pendingApprovedFeatureRecovery == nil
    }

    var body: some View {
        Section("Approved feature authoring") {
            DisclosureGroup("Author an approved feature") {
                VStack(alignment: .leading, spacing: 12) {
                    GroupBox(ApprovedFeatureReviewBindingPresentation.groupTitle) {
                        VStack(alignment: .leading, spacing: 8) {
                            LabeledContent("Feature ID") {
                                TextField("UUID", text: $form.featureID)
                                    .textFieldStyle(.roundedBorder)
                            }
                            LabeledContent("Repository ID") {
                                TextField("UUID", text: $form.repositoryID)
                                    .textFieldStyle(.roundedBorder)
                            }
                            DisclosureGroup("Import repository onboarding receipt") {
                                VStack(alignment: .leading, spacing: 6) {
                                    TextEditor(text: $onboardingReceiptText)
                                        .font(.caption.monospaced())
                                        .frame(minHeight: 72)
                                        .overlay {
                                            RoundedRectangle(cornerRadius: 5)
                                                .stroke(.quaternary)
                                        }
                                        .onChange(of: onboardingReceiptText) {
                                            onboardingReceiptImportStatus = nil
                                        }
                                    HStack {
                                        Text(
                                            "Paste the compact, path-free receipt emitted by the Windows onboarding flow."
                                        )
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        Spacer()
                                        Button("Import receipt") {
                                            importOnboardingReceipt()
                                        }
                                    }
                                    if let status = onboardingReceiptImportStatus {
                                        Label(status.message, systemImage: status.systemImage)
                                            .font(.caption)
                                            .foregroundStyle(status.isSuccess ? .green : .red)
                                    }
                                    Text(
                                        "Import only prefills this form. It creates no repository grant or authority; Windows rechecks current state during enqueue."
                                    )
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                }
                                .padding(.top, 4)
                            }
                            LabeledContent("Specification revision") {
                                TextField("1", text: $form.specificationRevision)
                                    .textFieldStyle(.roundedBorder)
                            }
                            LabeledContent(ApprovedFeatureReviewBindingPresentation.providerLabel) {
                                Text(form.providerID).textSelection(.enabled)
                            }
                            LabeledContent(ApprovedFeatureReviewBindingPresentation.modelLabel) {
                                Text(form.modelID).textSelection(.enabled)
                            }
                            Label(
                                ApprovedFeatureReviewBindingPresentation.lockLabel,
                                systemImage: "lock.fill"
                            )
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            Text(ApprovedFeatureReviewBindingPresentation.explanation)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .accessibilityIdentifier(
                        ApprovedFeatureReviewBindingPresentation.accessibilityIdentifier
                    )

                    GroupBox("Approved specification") {
                        VStack(alignment: .leading, spacing: 8) {
                            TextField("Title", text: $form.title)
                            TextField("Outcome", text: $form.outcome)
                            TextField("Scope", text: $form.scope)
                            ApprovedFeatureListEditor(
                                label: "Acceptance IDs (ASCII tokens, one per line)",
                                text: $form.acceptance
                            )
                            ApprovedFeatureListEditor(
                                label: "Allowed repository paths (one per line)",
                                text: $form.allowedPaths
                            )
                            ApprovedFeatureListEditor(
                                label: "Dependencies (feature UUIDs, one per line)",
                                text: $form.dependencies
                            )
                        }
                    }

                    GroupBox("Approval evidence") {
                        VStack(alignment: .leading, spacing: 8) {
                            TextField("Design SHA-256", text: $form.designSHA256)
                            TextField("Brainstorming SHA-256", text: $form.brainstormingSHA256)
                            TextField("Owner approval SHA-256", text: $form.ownerApprovalSHA256)
                            Text(
                                "Evidence fields accept lowercase or uppercase SHA-256 hex and are encoded as digest bytes. Raw design or brainstorming content is never sent."
                            )
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                    }

                    GroupBox("Current repository grants") {
                        VStack(alignment: .leading, spacing: 8) {
                            LabeledContent("Registration revision") {
                                TextField("1", text: $form.registrationGrantRevision)
                            }
                            LabeledContent("Cloud disclosure revision") {
                                TextField("1", text: $form.cloudDisclosureGrantRevision)
                            }
                            LabeledContent("Autonomous publication revision") {
                                TextField("1", text: $form.autonomousPublicationGrantRevision)
                            }
                            Text(
                                "This form does not create grants. Windows rechecks all three independent revisions before enqueue."
                            )
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                    }

                    DisclosureGroup("Specification obligations and publication") {
                        VStack(alignment: .leading, spacing: 8) {
                            ApprovedFeatureListEditor(label: "Assumptions", text: $form.assumptions)
                            ApprovedFeatureListEditor(label: "Risks", text: $form.risks)
                            ApprovedFeatureListEditor(label: "Non-goals", text: $form.nonGoals)
                            ApprovedFeatureListEditor(label: "Decisions", text: $form.decisions)
                            ApprovedFeatureListEditor(
                                label: "Required capabilities", text: $form.requiredCapabilities
                            )
                            ApprovedFeatureListEditor(
                                label: "Unit-test obligations", text: $form.unitTestObligations
                            )
                            ApprovedFeatureListEditor(
                                label: "Native E2E scenarios", text: $form.e2eScenarios
                            )
                            ApprovedFeatureListEditor(
                                label: "Documentation obligations",
                                text: $form.documentationObligations
                            )
                            ApprovedFeatureListEditor(
                                label: "Knowledge-base obligations",
                                text: $form.knowledgeBaseObligations
                            )
                            ApprovedFeatureListEditor(
                                label: "Prohibited data declarations", text: $form.prohibitedData
                            )
                            ApprovedFeatureListEditor(
                                label: "Required publication checks", text: $form.publicationChecks
                            )
                            TextField("Base branch", text: $form.baseBranch)
                            TextField(
                                "Security classification", text: $form.securityClassification
                            )
                            TextField("Merge strategy", text: $form.mergeStrategy)
                            TextField("Post-merge gate", text: $form.postMergeGate)
                            Text(
                                "The fixed 13-command validation gate is included automatically and cannot be edited here."
                            )
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                    }

                    HStack {
                        Button("Generate a new feature ID") {
                            form.featureID = UUID().uuidString.lowercased()
                        }
                        Spacer()
                        Button("Review and enqueue…") {
                            confirmationRequest = preparedRequest
                        }
                        .disabled(!authoringAvailable || preparedRequest == nil)
                    }
                }
                .padding(.top, 8)
            }

            if let receipt = model.approvedFeatureReceipt {
                LabeledContent(
                    "Last enqueue",
                    value: "\(receipt.featureID.uuidString.lowercased().prefix(8)) · queue r\(receipt.queueRevision)"
                )
            }
            if let recovery = model.pendingApprovedFeatureRecovery {
                let summary = ApprovedFeatureConfirmationSummary(
                    preparedRequest: recovery.preparedRequest
                )
                VStack(alignment: .leading, spacing: 6) {
                    Text("Exact enqueue reconciliation required")
                        .font(.headline)
                    Text(
                        "A prior enqueue may have committed without a valid observed receipt. New submissions are blocked. Reconciliation resends only the retained byte-identical request; it never rebuilds against a newer snapshot."
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    if let summary {
                        Text(summary.compactIdentity)
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                    }
                    Text(
                        "Exact request SHA-256: \(Self.hex(recovery.exactRequestSHA256))"
                    )
                    .font(.caption.monospaced())
                    .textSelection(.enabled)
                    Button("Reconcile exact enqueue…") {
                        reconciliationConfirmationPresented = true
                    }
                }
            }
            if let error = model.ownerActionErrorCode {
                LabeledContent("Owner action status", value: error)
            }
            Text(
                "Submission requires an authenticated current snapshot and a second explicit confirmation. The app stops observation, revalidates the exact signed helper, invokes only approve-and-enqueue --confirm, validates the redacted receipt, and restarts observation. Windows remains the canonical manifest-digest and queue authority."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .confirmationDialog(
            "Approve and enqueue this feature?",
            isPresented: Binding(
                get: { confirmationRequest != nil },
                set: { if !$0 { confirmationRequest = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Approve and enqueue") {
                guard let approved = confirmationRequest else { return }
                confirmationRequest = nil
                Task {
                    await model.performApprovedFeatureEnqueue(approved)
                    if model.approvedFeatureReceipt?.featureID == approved.draft.featureID {
                        form.resetAfterSuccessfulEnqueue()
                    }
                }
            }
            Button("Cancel", role: .cancel) { confirmationRequest = nil }
        } message: {
            if let confirmationRequest,
               let summary = ApprovedFeatureConfirmationSummary(
                preparedRequest: confirmationRequest
               ) {
                Text(summary.message)
            }
        }
        .confirmationDialog(
            "Reconcile the exact prior enqueue?",
            isPresented: $reconciliationConfirmationPresented,
            titleVisibility: .visible
        ) {
            Button("Reconcile exact enqueue") {
                reconciliationConfirmationPresented = false
                guard let frozen = model.pendingApprovedFeatureRecovery?.draft else { return }
                Task {
                    await model.reconcilePendingApprovedFeatureEnqueue()
                    if model.approvedFeatureReceipt?.featureID == frozen.featureID,
                       model.pendingApprovedFeatureRecovery == nil {
                        form.resetAfterSuccessfulEnqueue()
                    }
                }
            }
            Button("Cancel", role: .cancel) {
                reconciliationConfirmationPresented = false
            }
        } message: {
            if let recovery = model.pendingApprovedFeatureRecovery,
               let summary = ApprovedFeatureConfirmationSummary(
                preparedRequest: recovery.preparedRequest
               ) {
                Text(
                    "No fields or revisions will be rebuilt. The signed helper will receive the retained request with SHA-256 \(Self.hex(recovery.exactRequestSHA256)).\n\n\(summary.message)"
                )
            }
        }
    }

    private static func hex(_ bytes: [UInt8]) -> String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }

    private func importOnboardingReceipt() {
        do {
            try form.importRepositoryOnboardingReceipt(onboardingReceiptText)
            onboardingReceiptImportStatus = .success
        } catch AssemblywrightMacRepositoryOnboardingReceiptError.tooLarge {
            onboardingReceiptImportStatus = .failure(
                "Receipt is too large. Paste one compact Windows onboarding receipt."
            )
        } catch {
            onboardingReceiptImportStatus = .failure(
                "Receipt is invalid. Copy the complete onboarding receipt from Windows."
            )
        }
    }
}

enum ApprovedFeatureReviewBindingPresentation {
    static let groupTitle = "Identity and production review binding"
    static let providerLabel = "Production review provider"
    static let modelLabel = "Production review model"
    static let lockLabel = "Fixed by Windows master"
    static let accessibilityIdentifier = "approved-feature-production-review-binding"
    static let explanation =
        "This Feature Conveyor binding is separate from repository-scoped Codex development reviewer agents."
}

private enum OnboardingReceiptImportStatus: Equatable {
    case success
    case failure(String)

    var isSuccess: Bool {
        if case .success = self { return true }
        return false
    }

    var message: String {
        switch self {
        case .success:
            "Repository ID and current grant revisions imported."
        case let .failure(message):
            message
        }
    }

    var systemImage: String {
        isSuccess ? "checkmark.circle.fill" : "exclamationmark.triangle.fill"
    }
}

struct ApprovedFeatureConfirmationSummary: Equatable {
    static let maximumMessageBytes = 16 * 1_024
    let featureID: String
    let repositoryID: String
    let title: String
    let outcome: String
    let manifestSHA256: String
    let grants: String
    let providerModel: String
    let dependencyIDs: [String]
    let designDigestPrefix: String
    let brainstormingDigestPrefix: String
    let ownerApprovalDigestPrefix: String
    let queueRevision: UInt64
    let ownerControlDesignationRevision: UInt64
    let deviceID: String
    let connectionEpoch: UInt64
    let emergencyPaused: Bool
    let emergencyPauseRevision: UInt64
    let exactRequestSHA256: String

    init?(preparedRequest: AssemblywrightMacApprovedFeaturePreparedRequest) {
        let draft = preparedRequest.draft
        guard let manifestDigest = try? draft.manifestSHA256() else { return nil }
        featureID = draft.featureID.uuidString.lowercased()
        repositoryID = draft.repositoryID.uuidString.lowercased()
        title = draft.manifest.title ?? "(none)"
        outcome = draft.manifest.outcome
        manifestSHA256 = Self.hex(manifestDigest)
        grants = "registration r\(draft.grants.registration), cloud disclosure r\(draft.grants.cloudDisclosure), autonomous publication r\(draft.grants.autonomousPublication)"
        providerModel = "\(draft.providerID) / \(draft.modelID)"
        dependencyIDs = draft.dependencies.map { $0.uuidString.lowercased() }
        designDigestPrefix = Self.prefix(draft.designSHA256)
        brainstormingDigestPrefix = Self.prefix(draft.brainstormingSHA256)
        ownerApprovalDigestPrefix = Self.prefix(draft.ownerApprovalSHA256)
        queueRevision = preparedRequest.expectedQueueRevision
        ownerControlDesignationRevision = preparedRequest.ownerControlDesignationRevision
        deviceID = preparedRequest.deviceID
        connectionEpoch = preparedRequest.connectionEpoch
        emergencyPaused = preparedRequest.emergencyPaused
        emergencyPauseRevision = preparedRequest.emergencyPauseRevision
        exactRequestSHA256 = Self.hex(preparedRequest.exactRequestSHA256)
        guard message.utf8.count <= Self.maximumMessageBytes else { return nil }
    }

    var compactIdentity: String {
        "feature \(featureID) · repository \(repositoryID)"
    }

    var message: String {
        let dependencies = dependencyIDs.isEmpty
            ? "0 (none)"
            : "\(dependencyIDs.count): \(dependencyIDs.joined(separator: ", "))"
        return """
        Feature ID: \(featureID)
        Repository ID: \(repositoryID)
        Title: \(title)
        Outcome: \(outcome)
        Manifest SHA-256: \(manifestSHA256)
        Grants: \(grants)
        Review: \(providerModel)
        Dependencies: \(dependencies)
        Design digest: \(designDigestPrefix)…
        Brainstorming digest: \(brainstormingDigestPrefix)…
        Owner-approval digest: \(ownerApprovalDigestPrefix)…
        Queue revision: \(queueRevision)
        Owner-control designation: r\(ownerControlDesignationRevision), device \(deviceID), connection epoch \(connectionEpoch)
        Emergency Pause: \(emergencyPaused ? "active" : "inactive"), revision \(emergencyPauseRevision)
        Exact request SHA-256: \(exactRequestSHA256)
        """
    }

    private static func prefix(_ bytes: [UInt8]) -> String {
        String(hex(bytes).prefix(12))
    }

    private static func hex(_ bytes: [UInt8]) -> String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }
}

private struct ApprovedFeatureListEditor: View {
    let label: String
    @Binding var text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(.caption).foregroundStyle(.secondary)
            TextEditor(text: $text)
                .font(.body.monospaced())
                .frame(minHeight: 56)
                .overlay {
                    RoundedRectangle(cornerRadius: 5)
                        .stroke(.quaternary)
                }
        }
    }
}

struct ApprovedFeatureAuthoringForm: Equatable {
    var featureID = UUID().uuidString.lowercased()
    var repositoryID = ""
    var specificationRevision = "1"
    var providerID: String {
        AssemblywrightMacFeatureConveyorApprovedFeatureDraft.expectedProviderID
    }
    var modelID: String {
        AssemblywrightMacFeatureConveyorApprovedFeatureDraft.expectedModelID
    }
    var title = ""
    var outcome = ""
    var scope = ""
    var acceptance = ""
    var allowedPaths = ""
    var dependencies = ""
    var designSHA256 = ""
    var brainstormingSHA256 = ""
    var ownerApprovalSHA256 = ""
    var registrationGrantRevision = "1"
    var cloudDisclosureGrantRevision = "1"
    var autonomousPublicationGrantRevision = "1"
    var assumptions = ""
    var risks = ""
    var nonGoals = ""
    var decisions = ""
    var requiredCapabilities = ""
    var unitTestObligations = ""
    var e2eScenarios = ""
    var documentationObligations = ""
    var knowledgeBaseObligations = ""
    var prohibitedData = ""
    var publicationChecks = "release-local\nprotocol-windows"
    var baseBranch = "main"
    var securityClassification = "public"
    var mergeStrategy = "merge"
    var postMergeGate = "release-local"

    func draft() -> AssemblywrightMacFeatureConveyorApprovedFeatureDraft? {
        guard let featureID = strictUUID(featureID),
              let repositoryID = strictUUID(repositoryID),
              let specificationRevision = positiveRevision(specificationRevision),
              let designSHA256 = digest(designSHA256),
              let brainstormingSHA256 = digest(brainstormingSHA256),
              let ownerApprovalSHA256 = digest(ownerApprovalSHA256),
              let registration = positiveRevision(registrationGrantRevision),
              let cloudDisclosure = positiveRevision(cloudDisclosureGrantRevision),
              let autonomousPublication = positiveRevision(
                autonomousPublicationGrantRevision
              ),
              !outcome.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !lines(acceptance).isEmpty else { return nil }
        let acceptance = lines(acceptance)
        let dependencyLines = lines(self.dependencies)
        let dependencies = dependencyLines.compactMap(strictUUID)
        guard Set(acceptance).count == acceptance.count,
              dependencies.count == dependencyLines.count,
              Set(dependencies).count == dependencies.count,
              !dependencies.contains(featureID) else { return nil }
        let draft = AssemblywrightMacFeatureConveyorApprovedFeatureDraft(
            featureID: featureID,
            repositoryID: repositoryID,
            specificationRevision: specificationRevision,
            manifest: AssemblywrightMacApprovedFeatureManifest(
                acceptance: acceptance,
                outcome: outcome,
                title: optional(title),
                scope: optional(scope),
                allowedPaths: lines(allowedPaths),
                assumptions: lines(assumptions),
                risks: lines(risks),
                nonGoals: lines(nonGoals),
                decisions: lines(decisions),
                requiredCapabilities: lines(requiredCapabilities),
                unitTestObligations: lines(unitTestObligations),
                e2eScenarios: lines(e2eScenarios),
                documentationObligations: lines(documentationObligations),
                knowledgeBaseObligations: lines(knowledgeBaseObligations),
                prohibitedData: lines(prohibitedData),
                publicationChecks: lines(publicationChecks),
                baseBranch: optional(baseBranch),
                securityClassification: optional(securityClassification),
                mergeStrategy: optional(mergeStrategy),
                postMergeGate: optional(postMergeGate)
            ),
            designSHA256: designSHA256,
            brainstormingSHA256: brainstormingSHA256,
            ownerApprovalSHA256: ownerApprovalSHA256,
            grants: AssemblywrightMacFeatureConveyorGrantRevisions(
                registration: registration,
                cloudDisclosure: cloudDisclosure,
                autonomousPublication: autonomousPublication
            ),
            providerID: providerID,
            modelID: modelID,
            dependencies: dependencies
        )
        guard (try? draft.canonicalManifestData()) != nil else { return nil }
        return draft
    }

    func preparedRequest(
        from status: AssemblywrightDeveloperBridgeAppStatus
    ) -> AssemblywrightMacApprovedFeaturePreparedRequest? {
        guard let draft = draft() else { return nil }
        return try? draft.prepareRequest(from: status)
    }

    mutating func importRepositoryOnboardingReceipt(_ text: String) throws {
        let receipt = try AssemblywrightMacRepositoryOnboardingReceipt.decodeStrict(Data(text.utf8))
        repositoryID = receipt.repositoryID.uuidString.lowercased()
        registrationGrantRevision = String(receipt.registrationGrantRevision)
        cloudDisclosureGrantRevision = String(receipt.cloudDisclosureGrantRevision)
        autonomousPublicationGrantRevision = String(receipt.autonomousPublicationGrantRevision)
    }

    mutating func resetAfterSuccessfulEnqueue() {
        featureID = UUID().uuidString.lowercased()
        title = ""
        outcome = ""
        scope = ""
        acceptance = ""
        allowedPaths = ""
        dependencies = ""
        designSHA256 = ""
        brainstormingSHA256 = ""
        ownerApprovalSHA256 = ""
        assumptions = ""
        risks = ""
        nonGoals = ""
        decisions = ""
        requiredCapabilities = ""
        unitTestObligations = ""
        e2eScenarios = ""
        documentationObligations = ""
        knowledgeBaseObligations = ""
        prohibitedData = ""
    }

    private func lines(_ value: String) -> [String] {
        value.split(whereSeparator: { $0.isNewline })
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }

    private func optional(_ value: String) -> String? {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private func strictUUID(_ value: String) -> UUID? {
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard let uuid = UUID(uuidString: normalized),
              uuid != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) else {
            return nil
        }
        return uuid
    }

    private func positiveRevision(_ value: String) -> UInt64? {
        guard let revision = UInt64(value), revision > 0 else { return nil }
        return revision
    }

    private func digest(_ value: String) -> [UInt8]? {
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard normalized.count == 64,
              normalized.allSatisfy({ $0.isHexDigit }) else { return nil }
        let bytes = stride(from: 0, to: 64, by: 2).compactMap { offset in
            UInt8(normalized.dropFirst(offset).prefix(2), radix: 16)
        }
        guard bytes.count == 32, bytes.contains(where: { $0 != 0 }) else { return nil }
        return bytes
    }
}
