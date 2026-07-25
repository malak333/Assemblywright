import AppKit
import AssemblywrightMacCore
import SwiftUI

enum AssemblywrightMenuBarContract {
    static let mainWindowID = "assemblywright-main"
    static let title = "Assemblywright"
}

struct AssemblywrightMenuBarPresentation: Equatable {
    let statusLine: String
    let systemImage: String

    /// Whether the menu bar draws a state badge beside the proofmark. A
    /// connected bridge shows the mark alone; every other phase earns an
    /// indicator.
    let showsStateBadge: Bool

    init(status: AssemblywrightDeveloperBridgeAppStatus) {
        switch status.phase {
        case .disabled:
            statusLine = "Developer Mode disabled"
            systemImage = "circle"
            showsStateBadge = true
        case .starting:
            statusLine = "Bridge starting"
            systemImage = "circle.dotted"
            showsStateBadge = true
        case .connected:
            statusLine = "Bridge connected"
            systemImage = "checkmark.circle.fill"
            showsStateBadge = false
        case .masterOffline:
            statusLine = "Master offline"
            systemImage = "exclamationmark.triangle.fill"
            showsStateBadge = true
        case .maintenance:
            statusLine = "Master maintenance"
            systemImage = "wrench.and.screwdriver.fill"
            showsStateBadge = true
        case .paused:
            statusLine = "Bridge paused"
            systemImage = "pause.circle.fill"
            showsStateBadge = true
        case .stopped:
            statusLine = "Bridge stopped"
            systemImage = "circle"
            showsStateBadge = true
        }
    }
}

struct AssemblywrightMenuBarView: View {
    @ObservedObject var developerBridge: AssemblywrightDeveloperBridgeProcessLifecycle
    @Environment(\.openWindow) private var openWindow

    private var presentation: AssemblywrightMenuBarPresentation {
        AssemblywrightMenuBarPresentation(status: developerBridge.status)
    }

    var body: some View {
        Button("Open Assemblywright") {
            openWindow(id: AssemblywrightMenuBarContract.mainWindowID)
            NSApp.setActivationPolicy(.regular)
            NSApp.activate(ignoringOtherApps: true)
        }

        Divider()

        Text(presentation.statusLine)

        Divider()

        Button("Quit Assemblywright") {
            NSApplication.shared.terminate(nil)
        }
    }
}
