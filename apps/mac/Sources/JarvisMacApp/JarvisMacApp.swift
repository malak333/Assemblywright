import JarvisMacCore
import AppKit
import SwiftUI

@main
struct AssemblywrightMacApp: App {
    @StateObject private var developerBridge: JarvisDeveloperBridgeProcessLifecycle

    init() {
        _developerBridge = StateObject(wrappedValue: JarvisDeveloperBridgeProcessLifecycle())
    }

    var body: some Scene {
        WindowGroup("Assemblywright", id: JarvisMenuBarContract.mainWindowID) {
            AssemblywrightShellView(developerBridge: developerBridge)
                .background(AppActivationView())
                .task {
                    await developerBridge.superviseUntilCancelled()
                }
        }

        MenuBarExtra {
            JarvisMenuBarView(developerBridge: developerBridge)
        } label: {
            JarvisMenuBarLabel(
                presentation: JarvisMenuBarPresentation(status: developerBridge.status)
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
    @ObservedObject var developerBridge: JarvisDeveloperBridgeProcessLifecycle

    var body: some View {
        DeveloperBridgeStatusView(model: developerBridge)
            .frame(minWidth: 640, minHeight: 420)
    }
}

struct DeveloperBridgeStatusView: View {
    @ObservedObject var model: JarvisDeveloperBridgeProcessLifecycle

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

                Text(JarvisDeveloperBridgeProcessLifecycle.proofBoundary)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                if model.status.phase == .disabled {
                    Text(
                        "Development opt-in is disabled. Set \(JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey) to the exact separately signed helper and \(JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey) to its independently verified Apple team before launching Assemblywright."
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

    init(status: JarvisDeveloperBridgeAppStatus) {
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
