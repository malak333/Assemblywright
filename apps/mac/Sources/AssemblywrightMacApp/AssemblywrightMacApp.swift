import AssemblywrightMacCore
import AppKit
import SwiftUI

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
        }
        .formStyle(.grouped)
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
