import AssemblywrightMacCore
import AppKit
import SwiftUI
import UniformTypeIdentifiers

@main
struct AssemblywrightMacApp: App {
    @StateObject private var developerBridge: AssemblywrightDeveloperBridgeProcessLifecycle

    init() {
        _developerBridge = StateObject(wrappedValue: AssemblywrightDeveloperBridgeProcessLifecycle())
    }

    var body: some Scene {
        WindowGroup("Assemblywright", id: AssemblywrightMenuBarContract.mainWindowID) {
            AssemblywrightShellView(developerBridge: developerBridge)
                .background(AppActivationView())
                .task {
                    await developerBridge.superviseUntilCancelled()
                }
        }

        MenuBarExtra {
            AssemblywrightMenuBarView(developerBridge: developerBridge)
        } label: {
            AssemblywrightMenuBarLabel(
                presentation: AssemblywrightMenuBarPresentation(status: developerBridge.status)
            )
        }
        .menuBarExtraStyle(.menu)
    }
}

private struct AppActivationView: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            NSApp.setActivationPolicy(.regular)
            NSApp.activate(ignoringOtherApps: true)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}

struct AssemblywrightShellView: View {
    @ObservedObject var developerBridge: AssemblywrightDeveloperBridgeProcessLifecycle

    var body: some View {
        DeveloperBridgeStatusView(model: developerBridge)
            .frame(minWidth: 640, minHeight: 420)
    }
}

struct DeveloperBridgeStatusView: View {
    @ObservedObject var model: AssemblywrightDeveloperBridgeProcessLifecycle
    @State private var pendingAction: AssemblywrightMacOwnerControlAction?
    @State private var reconciliationDigest = ""
    @State private var mergedBeforeAbandon = false
    @State private var healthyMainDigest = ""
    @State private var localModelID = ""
    @State private var localModelExecutablePath = ""
    @State private var localModelDirectoryPath = ""
    @State private var pendingModelConfirmation: LocalModelConfirmation?
    @State private var choosingModelExecutable = false
    @State private var choosingModelDirectory = false

    private enum LocalModelConfirmation { case select, resume }

    private var presentation: DeveloperBridgeStatusPresentation {
        DeveloperBridgeStatusPresentation(status: model.status)
    }

    var body: some View {
        Form {
            Section("Windows-primary Developer Mode") {
                LabeledContent("Bridge", value: presentation.phaseLabel)

                if let endpoint = model.status.masterEndpoint,
                   let epoch = model.status.connectionEpoch {
                    LabeledContent("Master", value: endpoint)
                    LabeledContent("Connection epoch", value: String(epoch))
                }

                if let errorCode = model.status.errorCode {
                    LabeledContent("Status code", value: errorCode)
                }

                Text(AssemblywrightDeveloperBridgeProcessLifecycle.proofBoundary)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                if model.status.phase == .disabled {
                    Text(
                        "Development opt-in is disabled. Set \(AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey) to the exact separately signed helper and \(AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey) to its independently verified Apple team before launching Assemblywright."
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            }

            if let status = model.status.featureConveyor {
                let conveyor = FeatureConveyorStatusPresentation(status: status)
                Section("Feature Conveyor") {
                    LabeledContent("Queue", value: conveyor.queueLabel)
                    LabeledContent("State", value: conveyor.stateLabel)
                    LabeledContent("Guidance", value: conveyor.guidanceLabel)
                    if let currentFeatureLabel = conveyor.currentFeatureLabel {
                        LabeledContent("Current feature", value: currentFeatureLabel)
                    }
                    Text("Read-only observation. Guidance is not an approval or callable action.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            if let control = model.status.ownerControl {
                let owner = FeatureConveyorOwnerControlPresentation(control: control)
                Section("Owner control and activation") {
                    LabeledContent("Activation", value: owner.activationLabel)
                    LabeledContent("Blocker", value: owner.blockerLabel)
                    LabeledContent("Evidence", value: owner.evidenceLabel)
                    if let feature = owner.activeFeatureLabel {
                        LabeledContent("Active feature", value: feature)
                    }
                    ForEach(owner.evidenceDigests, id: \.self) { digest in
                        Text(digest).font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                    if control.activationReady {
                        Button("Activate Feature Conveyor…") { pendingAction = .activation }
                    }
                    if let feature = control.activeFeature {
                        if feature.ownerPaused || feature.stage != .paused {
                            Button(feature.ownerPaused ? "Resume orchestration…" : "Pause orchestration…") {
                                pendingAction = feature.ownerPaused ? .resume : .pause
                            }
                        }
                        Button("Cancel active feature…", role: .destructive) { pendingAction = .cancelActiveFeature }
                        if [.cancelled, .quarantined, .attentionRequired, .failed].contains(feature.lifecycleStatus) {
                            TextField("Safe reconciliation SHA-256", text: $reconciliationDigest)
                            Toggle("Candidate was merged", isOn: $mergedBeforeAbandon)
                            if mergedBeforeAbandon {
                                TextField("Verified healthy-main SHA-256", text: $healthyMainDigest)
                            }
                            Button("Abandon and advance…", role: .destructive) { pendingAction = .abandonAndAdvance }
                                .disabled(Self.digest(reconciliationDigest) == nil || (mergedBeforeAbandon && Self.digest(healthyMainDigest) == nil))
                        }
                    }
                    Text("Every action stops and reaps observation, revalidates the same signed helper, runs one explicit --confirm command, validates its receipt, then restarts observation. Stale revisions fail closed on Windows.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            let models = LocalModelSelectionPresentation(
                state: model.localModelSelectionState
            )
            Section("Models") {
                GroupBox("Mac MLX") {
                    LabeledContent("Status", value: models.macStatus)
                    if let active = model.localModelSelectionState.active {
                        LabeledContent("Active model", value: active.modelID)
                    }
                    if model.localModelSelectionState.pending == nil {
                        TextField("Model ID", text: $localModelID)
                        HStack {
                            TextField("mlx_lm.generate executable", text: $localModelExecutablePath)
                            Button("Browse…") { choosingModelExecutable = true }
                        }
                        HStack {
                            TextField("Canonical model directory", text: $localModelDirectoryPath)
                            Button("Browse…") { choosingModelDirectory = true }
                        }
                        Button("Apply model…") { pendingModelConfirmation = .select }
                            .disabled(
                                model.status.phase != .connected
                                    || localModelID.isEmpty
                                    || localModelExecutablePath.isEmpty
                                    || localModelDirectoryPath.isEmpty
                            )
                    } else {
                        Text("An ambiguous prior command blocks supervision until exact Windows reconciliation is separately confirmed.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Button("Resume exact selection…") {
                            pendingModelConfirmation = .resume
                        }
                    }
                }
                GroupBox("Local coding") {
                    LabeledContent("Model", value: models.localCoding)
                }
                GroupBox("Windows RTX") {
                    LabeledContent("Status", value: models.windowsRTX)
                }
                GroupBox("Production review") {
                    LabeledContent("Provider", value: models.productionReview)
                }
                Text("Only the Mac MLX model is selectable. Local paths remain in the owner-private Mac store and never enter Windows status or audit.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                if let error = model.localModelSelectionErrorCode {
                    LabeledContent("Model selection code", value: error)
                }
            }

            ApprovedFeatureAuthoringSection(model: model)
        }
        .formStyle(.grouped)
        .disabled(model.ownerActionInProgress)
        .confirmationDialog(
            "Confirm Windows-authoritative owner action",
            isPresented: Binding(get: { pendingAction != nil }, set: { if !$0 { pendingAction = nil } }),
            titleVisibility: .visible
        ) {
            Button("Confirm", role: pendingAction == .cancelActiveFeature ? .destructive : nil) {
                guard let action = pendingAction else { return }
                pendingAction = nil
                Task {
                    await model.performOwnerAction(
                        action,
                        safeReconciliationSHA256: Self.digest(reconciliationDigest),
                        merged: mergedBeforeAbandon,
                        verifiedHealthyMainSHA256: Self.digest(healthyMainDigest)
                    )
                }
            }
            Button("Cancel", role: .cancel) { pendingAction = nil }
        } message: {
            Text("The current queue, designation, lifecycle, orchestration, and Emergency Pause revisions are included. This cannot resume Emergency Pause.")
        }
        .confirmationDialog(
            "Confirm local model selection",
            isPresented: Binding(
                get: { pendingModelConfirmation != nil },
                set: { if !$0 { pendingModelConfirmation = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Confirm") {
                guard let confirmation = pendingModelConfirmation else { return }
                pendingModelConfirmation = nil
                Task {
                    switch confirmation {
                    case .select:
                        await model.selectLocalModel(
                            modelID: localModelID,
                            executablePath: localModelExecutablePath,
                            modelDirectoryPath: localModelDirectoryPath
                        )
                    case .resume:
                        await model.resumePendingLocalModelSelection()
                    }
                }
            }
            Button("Cancel", role: .cancel) { pendingModelConfirmation = nil }
        } message: {
            Text("Change \(model.status.localModelSelection?.modelID ?? "current model") to \(localModelID.isEmpty ? model.localModelSelectionState.pending?.configuration.modelID ?? "pending model" : localModelID). Windows will advance registry and designation revisions and disconnect the old session.")
        }
        .fileImporter(
            isPresented: $choosingModelExecutable,
            allowedContentTypes: [.item],
            allowsMultipleSelection: false
        ) { result in
            if case let .success(urls) = result, let url = urls.first {
                localModelExecutablePath = url.path
            }
        }
        .fileImporter(
            isPresented: $choosingModelDirectory,
            allowedContentTypes: [.folder],
            allowsMultipleSelection: false
        ) { result in
            if case let .success(urls) = result, let url = urls.first {
                localModelDirectoryPath = url.path
            }
        }
    }

    private static func digest(_ text: String) -> [UInt8]? {
        let normalized = text.lowercased()
        guard normalized.count == 64, normalized.allSatisfy({ $0.isHexDigit }) else { return nil }
        return stride(from: 0, to: 64, by: 2).compactMap { offset in
            UInt8(normalized.dropFirst(offset).prefix(2), radix: 16)
        }
    }
}

struct LocalModelSelectionPresentation: Equatable {
    let macStatus: String
    let localCoding: String
    let windowsRTX: String
    let productionReview: String

    init(state: AssemblywrightMacLocalModelSelectionState) {
        if let pending = state.pending {
            macStatus = "Pending reconciliation: \(pending.configuration.modelID)"
        } else if let active = state.active {
            macStatus = "Active: \(active.modelID)"
        } else {
            macStatus = "Uses installed MLX profile"
        }
        localCoding = "assemblywright-local-coding-v1 (fixed)"
        windowsRTX = "Not provisioned"
        productionReview = "openai.codex / gpt-5.6-sol (fixed)"
    }
}

struct FeatureConveyorOwnerControlPresentation: Equatable {
    let activationLabel: String
    let blockerLabel: String
    let evidenceLabel: String
    let activeFeatureLabel: String?
    let evidenceDigests: [String]

    init(control: AssemblywrightMacFeatureConveyorOwnerControlProjection) {
        activationLabel = control.activationStatus == .active ? "Active" : control.activationReady ? "Ready for owner confirmation" : "Inactive"
        switch control.activationBlocker {
        case .none: blockerLabel = "None"
        case .emergencyPaused: blockerLabel = "Emergency Pause"
        case .evidenceRequired: blockerLabel = "Evidence required"
        case .alreadyActivated: blockerLabel = "Already activated"
        }
        evidenceLabel = "\(control.evidence.readyCount) of 6 Windows-admitted categories ready"
        if let feature = control.activeFeature {
            activeFeatureLabel = "\(feature.featureID.uuidString.lowercased().prefix(8)) · \(feature.stage.rawValue)"
        } else { activeFeatureLabel = nil }
        let labels = ["repository", "worker", "review", "github", "restart", "control-stream"]
        evidenceDigests = zip(labels, control.evidence.referencesForPresentation).map { label, reference in
            guard let reference else { return "\(label): missing" }
            let digest = reference.receiptSHA256.prefix(4).map { String(format: "%02x", $0) }.joined()
            return "\(label): r\(reference.revision) \(digest)…"
        }
    }
}

struct FeatureConveyorStatusPresentation: Equatable {
    let queueLabel: String
    let stateLabel: String
    let guidanceLabel: String
    let currentFeatureLabel: String?

    init(status: AssemblywrightMacFeatureConveyorStatus) {
        queueLabel = "\(status.countsByStatus.queued) queued · \(status.visibleFeatureCount) visible"
        switch status.ownerGuidance.state {
        case .idle: stateLabel = "Idle"
        case .ready: stateLabel = "Ready"
        case .blocked: stateLabel = "Blocked"
        case .inProgress: stateLabel = "In progress"
        }
        switch status.ownerGuidance.nextOwnerAction {
        case .prepareApprovedFeature: guidanceLabel = "Prepare an approved feature"
        case .awaitOwnerControlSurface: guidanceLabel = "Await owner control surface"
        case .resolveHeadDependency: guidanceLabel = "Resolve the head dependency"
        case .wait: guidanceLabel = "Wait"
        case .reconcileActiveFeature: guidanceLabel = "Reconcile the active feature"
        case .resumeEmergencyPause: guidanceLabel = "Resume Emergency Pause deliberately"
        }
        if let featureID = status.ownerGuidance.featureID,
           let feature = status.features.first(where: { $0.featureID == featureID }) {
            let identifier = String(featureID.uuidString.lowercased().prefix(8))
            currentFeatureLabel = "\(identifier) · \(feature.status.rawValue)"
        } else {
            currentFeatureLabel = nil
        }
    }
}

struct DeveloperBridgeStatusPresentation: Equatable {
    let phaseLabel: String

    init(status: AssemblywrightDeveloperBridgeAppStatus) {
        switch status.phase {
        case .disabled:
            phaseLabel = "Disabled"
        case .starting:
            phaseLabel = "Starting"
        case .connected:
            phaseLabel = "Connected"
        case .masterOffline:
            phaseLabel = "Master Offline"
        case .maintenance:
            phaseLabel = "Maintenance"
        case .paused:
            phaseLabel = "Paused"
        case .stopped:
            phaseLabel = "Stopped"
        }
    }
}
