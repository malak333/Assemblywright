import AppKit
import JarvisMacCore
import SwiftUI

enum JarvisMenuBarContract {
    static let mainWindowID = "jarvis-main"
    static let title = "Jarvis"
}

struct JarvisMenuBarPresentation: Equatable {
    let statusLine: String
    let systemImage: String
    let canStartCore: Bool
    let canStopCore: Bool

    init(mode: JarvisCoreMode) {
        switch mode {
        case .stopped:
            statusLine = "Core stopped"
            systemImage = "circle"
            canStartCore = true
            canStopCore = false
        case .starting:
            statusLine = "Core starting"
            systemImage = "circle.dotted"
            canStartCore = false
            canStopCore = false
        case .available:
            statusLine = "Core available"
            systemImage = "checkmark.circle.fill"
            canStartCore = false
            canStopCore = true
        case .degraded:
            statusLine = "Core degraded"
            systemImage = "exclamationmark.triangle.fill"
            canStartCore = false
            canStopCore = true
        }
    }
}

struct JarvisMenuBarView: View {
    @ObservedObject var supervisor: JarvisCoreSupervisor
    @ObservedObject var console: CommandConsoleModel
    @ObservedObject var modelConfiguration: ModelConfigurationModel
    @Environment(\.openWindow) private var openWindow

    private var presentation: JarvisMenuBarPresentation {
        JarvisMenuBarPresentation(mode: supervisor.mode)
    }

    var body: some View {
        Button("Open Jarvis") {
            openWindow(id: JarvisMenuBarContract.mainWindowID)
            NSApp.setActivationPolicy(.regular)
            NSApp.activate(ignoringOtherApps: true)
        }

        Divider()

        Text(presentation.statusLine)

        Button("Refresh Health") {
            Task {
                await supervisor.refreshHealth()
                await synchronizeConsoleWithSupervisor(
                    supervisor: supervisor,
                    console: console,
                    modelConfiguration: modelConfiguration
                )
            }
        }

        Button("Start Core") {
            Task {
                await supervisor.start(
                    environmentOverrides: modelConfiguration.launchEnvironmentOverrides
                )
                await synchronizeConsoleWithSupervisor(
                    supervisor: supervisor,
                    console: console,
                    modelConfiguration: modelConfiguration
                )
            }
        }
        .disabled(!presentation.canStartCore)

        Button("Stop Core") {
            Task {
                guard await supervisor.stop() else {
                    console.markDegraded("Jarvis core did not stop before the shutdown timeout.")
                    return
                }
                console.markDegraded("Jarvis core was stopped from the menu bar.")
            }
        }
        .disabled(!presentation.canStopCore)

        Divider()

        Button("Quit Jarvis") {
            NSApplication.shared.terminate(nil)
        }
    }
}
