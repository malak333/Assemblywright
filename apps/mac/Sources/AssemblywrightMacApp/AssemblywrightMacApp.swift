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
        }
        .formStyle(.grouped)
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
